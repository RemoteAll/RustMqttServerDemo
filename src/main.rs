use rumqttd::{Broker, Config};
use log::{info, error};
use std::fs;
use std::path::Path;

mod mqtt_adapter;

#[tokio::main]
async fn main() {
    // 初始化日志
    // 注意: rumqttd 库使用 ERROR 级别记录内部消息流跟踪 (如 "[>] incoming")
    // 这是库的设计问题,不是真正的错误。这些消息表示正常的消息路由流程。
    // 如果想要清晰的日志,可以添加过滤: "rumqttd::router::routing=warn"
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();
    
    // 从配置文件加载配置
    let config = load_config("config.toml");
    
    info!("Starting MQTT Broker...");
    info!("Configuration loaded from: config.toml");
    info!("Listening on:");
    info!("  - TCP: 0.0.0.0:1882 (MQTT 3.1.0 - with adapter)");
    info!("  - TCP: 0.0.0.0:1883 (MQTT 3.1.1)");
    info!("  - TCP: 0.0.0.0:1884 (MQTT 5.0)");
    info!("  - WebSocket: 0.0.0.0:8080 (MQTT 3.1.1)");
    info!("  - Console: 0.0.0.0:3030 (Management)");
    
    // 启动 MQTT 3.1.0 适配器(异步)
    // 监听 1882 端口,转发到 1883 端口(MQTT 3.1.1)
    tokio::spawn(async {
        if let Err(e) = mqtt_adapter::start_mqtt31_adapter(1882, 1883).await {
            error!("MQTT 3.1.0 adapter failed: {}", e);
        }
    });
    
    // 启动 Broker (这是一个阻塞调用)
    let mut broker = Broker::new(config);
    
    match broker.start() {
        Ok(_) => info!("Broker stopped gracefully"),
        Err(e) => error!("Broker error: {}", e),
    }
}

/// 从文件加载配置
fn load_config(config_path: &str) -> Config {
    let path = Path::new(config_path);
    
    if !path.exists() {
        error!("Configuration file not found: {}", config_path);
        error!("Creating default configuration file...");
        create_default_config(config_path);
        info!("Default configuration created. Please edit {} and restart.", config_path);
        std::process::exit(1);
    }
    
    let config_content = fs::read_to_string(path)
        .unwrap_or_else(|e| {
            error!("Failed to read configuration file: {}", e);
            std::process::exit(1);
        });
    
    toml::from_str(&config_content)
        .unwrap_or_else(|e| {
            error!("Failed to parse configuration file: {}", e);
            std::process::exit(1);
        })
}

/// 创建默认配置文件
fn create_default_config(config_path: &str) {
    let default_config = r#"# MQTT Broker 配置文件
id = 0

[router]
max_segment_size = 104857600
max_segment_count = 10
max_connections = 10000
max_outgoing_packet_count = 200

# TCP 监听器 - MQTT 3.1.1
[v4.1]
name = "tcp-v4"
listen = "0.0.0.0:1883"
next_connection_delay_ms = 1
    
[v4.1.connections]
connection_timeout_ms = 60000
max_client_id_len = 256
max_connections = 10000
max_payload_size = 268435455
max_inflight_count = 100
throttle_delay_ms = 0

# TCP 监听器 - MQTT 5.0
[v5.1]
name = "tcp-v5"
listen = "0.0.0.0:1884"
next_connection_delay_ms = 1

[v5.1.connections]
connection_timeout_ms = 60000
max_client_id_len = 256
max_connections = 10000
max_payload_size = 268435455
max_inflight_count = 100
throttle_delay_ms = 0

# WebSocket 监听器 (MQTT 3.1.1)
[ws.1]
name = "ws-v4"
listen = "0.0.0.0:8080"
next_connection_delay_ms = 1

[ws.1.connections]
connection_timeout_ms = 60000
max_client_id_len = 256
max_connections = 10000
max_payload_size = 268435455
max_inflight_count = 100
throttle_delay_ms = 0

# 控制台配置 (用于监控和管理)
[console]
listen = "0.0.0.0:3030"
"#;
    
    fs::write(config_path, default_config)
        .unwrap_or_else(|e| {
            error!("Failed to create default configuration file: {}", e);
            std::process::exit(1);
        });
}
