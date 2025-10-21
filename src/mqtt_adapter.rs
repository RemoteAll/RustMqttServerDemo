// MQTT 3.1.0 到 3.1.1 协议适配器
// 用于兼容旧版 MQTT 3.1.0 客户端

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use log::{info, warn, debug};

/// 启动 MQTT 3.1.0 适配器监听器
/// 将 MQTT 3.1.0 协议升级为 3.1.1 后转发到主 broker
pub async fn start_mqtt31_adapter(listen_port: u16, forward_port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await?;
    info!("MQTT 3.1.0 adapter listening on 0.0.0.0:{} (forwards to 127.0.0.1:{})", listen_port, forward_port);
    
    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        debug!("MQTT 3.1.0 adapter: New connection from {}", client_addr);
        
        let forward_addr = format!("127.0.0.1:{}", forward_port);
        tokio::spawn(async move {
            if let Err(e) = handle_mqtt31_client(client_stream, forward_addr).await {
                warn!("MQTT 3.1.0 adapter error: {}", e);
            }
        });
    }
}

/// 处理单个 MQTT 3.1.0 客户端连接
async fn handle_mqtt31_client(mut client_stream: TcpStream, forward_addr: String) -> std::io::Result<()> {
    // 连接到真正的 MQTT broker
    let mut broker_stream = TcpStream::connect(&forward_addr).await?;
    
    // 读取客户端的 CONNECT 包
    let mut first_byte = [0u8; 1];
    client_stream.read_exact(&mut first_byte).await?;
    
    // 检查是否是 CONNECT 包 (固定头 0x10)
    if first_byte[0] >> 4 != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Expected CONNECT packet"
        ));
    }
    
    // 读取剩余长度
    let remaining_length = read_remaining_length(&mut client_stream).await?;
    
    // 读取完整的 CONNECT 包负载
    let mut payload = vec![0u8; remaining_length];
    client_stream.read_exact(&mut payload).await?;
    
    // 检查协议名称和版本
    if payload.len() < 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid CONNECT packet"
        ));
    }
    
    // MQTT 3.1.0 的协议名称是 "MQIsdp" (6 字节)
    // MQTT 3.1.1 的协议名称是 "MQTT" (4 字节)
    let protocol_name_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    
    if protocol_name_len == 6 && &payload[2..8] == b"MQIsdp" {
        // 这是 MQTT 3.1.0 客户端!
        info!("Detected MQTT 3.1.0 client, upgrading to 3.1.1");
        
        // 协议版本应该是 3
        if payload[8] == 3 {
            // 转换为 MQTT 3.1.1 格式
            let mut new_payload = Vec::new();
            
            // 新的协议名称: "MQTT" (4 字节)
            new_payload.extend_from_slice(&[0, 4]); // 长度
            new_payload.extend_from_slice(b"MQTT"); // 协议名
            new_payload.push(4); // MQTT 3.1.1 的协议级别是 4
            
            // 复制剩余的字段 (从连接标志开始)
            new_payload.extend_from_slice(&payload[9..]);
            
            // 重新计算剩余长度
            let new_remaining_length = new_payload.len();
            
            // 发送转换后的 CONNECT 包到 broker
            broker_stream.write_u8(first_byte[0]).await?;
            write_remaining_length(&mut broker_stream, new_remaining_length).await?;
            broker_stream.write_all(&new_payload).await?;
            broker_stream.flush().await?;
            
            debug!("Upgraded MQTT 3.1.0 CONNECT to 3.1.1");
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown MQIsdp version: {}", payload[8])
            ));
        }
    } else {
        // 不是 MQTT 3.1.0,直接转发原始数据
        broker_stream.write_u8(first_byte[0]).await?;
        write_remaining_length(&mut broker_stream, remaining_length).await?;
        broker_stream.write_all(&payload).await?;
        broker_stream.flush().await?;
    }
    
    // 双向转发剩余数据
    let (mut client_read, mut client_write) = client_stream.into_split();
    let (mut broker_read, mut broker_write) = broker_stream.into_split();
    
    let client_to_broker = tokio::spawn(async move {
        let mut buffer = [0u8; 8192];
        loop {
            match client_read.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if broker_write.write_all(&buffer[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    let broker_to_client = tokio::spawn(async move {
        let mut buffer = [0u8; 8192];
        loop {
            match broker_read.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if client_write.write_all(&buffer[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    // 等待任一方向关闭
    tokio::select! {
        _ = client_to_broker => {},
        _ = broker_to_client => {},
    }
    
    Ok(())
}

/// 读取 MQTT 剩余长度字段
async fn read_remaining_length(stream: &mut TcpStream) -> std::io::Result<usize> {
    let mut multiplier = 1;
    let mut value = 0;
    
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        
        value += ((byte[0] & 127) as usize) * multiplier;
        multiplier *= 128;
        
        if byte[0] & 128 == 0 {
            break;
        }
        
        if multiplier > 128 * 128 * 128 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid remaining length"
            ));
        }
    }
    
    Ok(value)
}

/// 写入 MQTT 剩余长度字段
async fn write_remaining_length(stream: &mut TcpStream, mut length: usize) -> std::io::Result<()> {
    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;
        
        if length > 0 {
            byte |= 128;
        }
        
        stream.write_u8(byte).await?;
        
        if length == 0 {
            break;
        }
    }
    
    Ok(())
}
