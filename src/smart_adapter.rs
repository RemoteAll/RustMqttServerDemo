// MQTT 多协议智能适配器
// 在单个端口上自动检测 MQTT 3.1.0, 3.1.1, 5.0 协议

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use log::{info, warn, debug, error};

/// MQTT 协议版本
#[derive(Debug, Clone, Copy)]
enum MqttVersion {
    V310,  // MQTT 3.1.0 (MQIsdp)
    V311,  // MQTT 3.1.1
    V500,  // MQTT 5.0
}

/// 启动智能 MQTT 适配器
/// 在单个端口上自动检测并处理所有 MQTT 版本
pub async fn start_smart_mqtt_adapter(
    listen_port: u16,
    forward_port: u16,  // 统一的 broker 端口
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await?;
    info!("Smart MQTT adapter listening on 0.0.0.0:{}", listen_port);
    info!("  - Auto-detects MQTT 3.1.0, 3.1.1, and 5.0");
    info!("  - Upgrades MQTT 3.1.0 to 3.1.1 transparently");
    
    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        debug!("Smart adapter: New connection from {}", client_addr);
        
        let forward_addr = format!("127.0.0.1:{}", forward_port);
        
        tokio::spawn(async move {
            if let Err(e) = handle_smart_client(client_stream, forward_addr).await {
                warn!("Smart adapter error: {}", e);
            }
        });
    }
}

/// 处理单个客户端连接,自动检测协议版本
async fn handle_smart_client(
    mut client_stream: TcpStream,
    forward_addr: String,
) -> std::io::Result<()> {
    // 读取 CONNECT 包的固定头
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
    
    // 检测协议版本
    let (mqtt_version, modified_payload) = detect_and_convert_protocol(&payload)?;
    
    // 记录协议版本
    let version_name = match mqtt_version {
        MqttVersion::V310 => {
            info!("Detected MQTT 3.1.0 client, upgrading to 3.1.1");
            "3.1.0→3.1.1"
        }
        MqttVersion::V311 => {
            info!("Detected MQTT 3.1.1 client");
            "3.1.1"
        }
        MqttVersion::V500 => {
            info!("Detected MQTT 5.0 client");
            "5.0"
        }
    };
    
    // 连接到 broker (rumqttd 会自动识别 3.1.1 和 5.0)
    let mut broker_stream = TcpStream::connect(&forward_addr).await
        .map_err(|e| {
            error!("Failed to connect to backend broker ({}): {}", version_name, e);
            e
        })?;
    
    // 发送(可能修改过的) CONNECT 包
    broker_stream.write_u8(first_byte[0]).await?;
    write_remaining_length(&mut broker_stream, modified_payload.len()).await?;
    broker_stream.write_all(&modified_payload).await?;
    broker_stream.flush().await?;
    
    debug!("Forwarded CONNECT packet to {} broker", version_name);
    
    // 双向转发剩余数据
    bidirectional_forward(client_stream, broker_stream).await?;
    
    Ok(())
}

/// 检测 MQTT 协议版本并转换 (如果需要)
/// 返回: (协议版本, 可能修改后的负载)
fn detect_and_convert_protocol(payload: &[u8]) -> std::io::Result<(MqttVersion, Vec<u8>)> {
    if payload.len() < 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "CONNECT packet too short"
        ));
    }
    
    // 读取协议名称长度
    let protocol_name_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    
    if payload.len() < 2 + protocol_name_len + 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid CONNECT packet"
        ));
    }
    
    let protocol_name = &payload[2..2 + protocol_name_len];
    let protocol_level = payload[2 + protocol_name_len];
    
    // 检测协议版本
    match (protocol_name, protocol_level) {
        // MQTT 3.1.0: MQIsdp, level 3
        (b"MQIsdp", 3) => {
            // 需要转换为 MQTT 3.1.1
            let mut new_payload = Vec::new();
            
            // 新的协议名称: "MQTT" (4 字节)
            new_payload.extend_from_slice(&[0, 4]);
            new_payload.extend_from_slice(b"MQTT");
            new_payload.push(4); // MQTT 3.1.1 协议级别
            
            // 复制剩余字段
            new_payload.extend_from_slice(&payload[2 + protocol_name_len + 1..]);
            
            Ok((MqttVersion::V310, new_payload))
        }
        
        // MQTT 3.1.1: MQTT, level 4
        (b"MQTT", 4) => {
            Ok((MqttVersion::V311, payload.to_vec()))
        }
        
        // MQTT 5.0: MQTT, level 5
        (b"MQTT", 5) => {
            Ok((MqttVersion::V500, payload.to_vec()))
        }
        
        _ => {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown MQTT protocol: {:?}, level {}", 
                    String::from_utf8_lossy(protocol_name), protocol_level)
            ))
        }
    }
}

/// 双向转发数据流
async fn bidirectional_forward(
    client_stream: TcpStream,
    broker_stream: TcpStream,
) -> std::io::Result<()> {
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
