# Rust MQTT Broker Demo

基于 `rumqttd` 的 MQTT Broker 实现,支持 MQTT 3.1.1 和 MQTT 5.0 协议。

## 功能特性

- ✅ **MQTT 3.1.0** 支持 (端口 1882 - 通过协议适配器)
- ✅ **MQTT 3.1.1** 支持 (端口 1883)
- ✅ **MQTT 5.0** 支持 (端口 1884)
- ✅ **WebSocket** 支持 (端口 8080)
- ✅ **管理控制台** (端口 3030)
- ✅ **可配置文件** 支持
- ✅ **高并发** 支持 (默认 10000 连接)
- ✅ **多协议版本** 同时运行
- ✅ **自动协议升级** MQTT 3.1.0 → 3.1.1

## 快速开始

### 1. 编译运行

```bash
cargo run
```

首次运行会自动生成 `config.toml` 配置文件。

### 2. 配置文件

编辑 `config.toml` 可以自定义:
- 端口号
- 最大连接数
- 超时设置
- 消息大小限制
- 等等...

### 3. 测试连接

#### 使用 mosquitto 客户端测试

**MQTT 3.1.0:**
```bash
# 订阅主题
mosquitto_sub -h localhost -p 1882 -t "test/topic" -V mqttv31

# 发布消息
mosquitto_pub -h localhost -p 1882 -t "test/topic" -m "Hello MQTT 3.1.0!" -V mqttv31
```

**MQTT 3.1.1:**
```bash
# 订阅主题
mosquitto_sub -h localhost -p 1883 -t "test/topic" -V mqttv311

# 发布消息
mosquitto_pub -h localhost -p 1883 -t "test/topic" -m "Hello MQTT 3.1.1!" -V mqttv311
```

**MQTT 5.0:**
```bash
# 订阅主题
mosquitto_sub -h localhost -p 1884 -t "test/topic" -V mqttv5

# 发布消息
mosquitto_pub -h localhost -p 1884 -t "test/topic" -m "Hello MQTT 5.0!" -V mqttv5
```

**WebSocket:**
```bash
# 使用 WebSocket 连接
mosquitto_sub -h localhost -p 8080 -t "test/topic" -V mqttv311
```

#### 使用 MQTTX 图形界面

1. 下载 [MQTTX](https://mqttx.app/)
2. 创建新连接:
   - Host: `localhost`
   - Port: `1883` (MQTT 3.1.1) 或 `1884` (MQTT 5.0)
   - Protocol: `mqtt://`

## 配置说明

### 端口配置

- **1882**: MQTT 3.1.0 协议 (通过适配器自动升级到 3.1.1)
- **1883**: MQTT 3.1.1 协议
- **1884**: MQTT 5.0 协议
- **8080**: WebSocket (MQTT 3.1.1)
- **3030**: 管理控制台

### 性能配置

```toml
[router]
max_segment_size = 104857600     # 最大段大小 (100MB)
max_segment_count = 10           # 最大段数量
max_connections = 10000          # 最大连接数
```

### 连接配置

```toml
[v4.1.connections]
connection_timeout_ms = 60000    # 连接超时 (60秒)
max_client_id_len = 256          # 客户端ID最大长度
max_connections = 10000          # 最大连接数
```

## 日志配置

设置日志级别:
```bash
# Windows PowerShell
$env:RUST_LOG="info"; cargo run

# Linux/Mac
RUST_LOG=debug cargo run
```

日志级别: `error`, `warn`, `info`, `debug`, `trace`

## 生产部署建议

1. **使用 Release 模式编译**:
   ```bash
   cargo build --release
   ./target/release/rustmqttserverdemo
   ```

2. **配置防火墙**:
   - 开放端口 1883, 1884, 8080

3. **配置 TLS/SSL** (生产环境必须):
   编辑 `config.toml` 添加证书配置

4. **性能优化**:
   - 根据实际需求调整 `max_connections`
   - 调整 `max_segment_size` 和 `max_segment_count`

## 故障排查

### 日志中的 ERROR 信息

你可能会看到类似这样的日志:
```
[ERROR rumqttd::router::routing] [>] incoming; connection_id=0
```

**这不是真正的错误!** 这是 `rumqttd` 库的一个设计问题,它使用 ERROR 级别记录内部消息流跟踪。这些消息表示正常的路由流程:
- `[>]` = 消息进入路由器
- `[<]` = 消息离开路由器

如果你想要更清晰的日志,可以设置环境变量:
```bash
$env:RUST_LOG="warn,rustmqttserverdemo=info,rumqttd::server::broker=info"
```

### 端口被占用

如果端口被占用,修改 `config.toml` 中的端口配置。

### 配置文件错误

删除 `config.toml`,重新运行程序会自动生成默认配置。

### 连接超时

增加 `connection_timeout_ms` 的值。

## 协议支持说明

### MQTT 3.1.0 支持

✅ **现已支持 MQTT 3.1.0!**

本 broker 通过内置的**协议适配器**支持 MQTT 3.1.0:
- 监听端口: **1882**
- 工作原理: 自动将 MQTT 3.1.0 (MQIsdp) 协议升级为 MQTT 3.1.1
- 完全透明: 客户端无需修改,就像连接原生 3.1.0 broker 一样
- 性能: 零拷贝转发,延迟极低

旧设备可以直接连接到端口 1882 使用 MQTT 3.1.0 协议!

## License

MIT
