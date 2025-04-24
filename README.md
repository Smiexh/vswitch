# VSwitch - 虚拟交换机

VSwitch 是一个基于 Rust 实现的虚拟交换机，它允许跳板机通过 TUN 网卡与多个客户端组成点对点网络。使用UDP协议通信，具有更低的延迟和开销。

## 特性

- 纯静态编译，使用 musl libc
- 单文件部署，通过命令行参数区分客户端和服务端
- 基于UDP协议，低延迟，适合隧道应用
- 支持点对点网络
- 适用于跳板机场景
- 代码结构清晰，易于扩展

## 构建

### 安装 Rust

如果您还没有安装 Rust，请按照 [Rust 官网](https://www.rust-lang.org/tools/install) 的指导进行安装。

### 安装 musl 工具链

```bash
# 在 Ubuntu/Debian 上:
apt-get install musl-tools

# 在 macOS 上 (通过 Homebrew):
brew install FiloSottile/musl-cross/musl-cross
```

### 添加目标平台

```bash
rustup target add x86_64-unknown-linux-musl
# 或者对于 ARM 架构:
rustup target add aarch64-unknown-linux-musl
```

### 编译

```bash
# 对于 x86_64 架构:
cargo build --release --target=x86_64-unknown-linux-musl

# 对于 ARM 架构:
cargo build --release --target=aarch64-unknown-linux-musl
```

编译完成后，可执行文件将位于 `target/x86_64-unknown-linux-musl/release/vswitch` 或 `target/aarch64-unknown-linux-musl/release/vswitch`。

## 使用方法

### 服务端模式

在跳板机上以服务端模式运行：

```bash
./vswitch server --listen 0.0.0.0:4789 --tun-name tun0 --mtu 1500
```

### 客户端模式

在客户端机器上运行：

```bash
./vswitch client --server 服务器IP:4789 --tun-name tun0 --mtu 1500
```

### 参数说明

- `--log-level`: 日志级别，可选值：error, warn, info, debug, trace，默认为 info
- `server`: 服务端子命令
  - `--listen, -l`: 监听地址，默认为 0.0.0.0:4789
  - `--tun-name, -t`: TUN 设备名称，默认为 tun0
  - `--mtu, -m`: MTU 大小，默认为 1500
- `client`: 客户端子命令
  - `--server, -s`: 服务器地址，格式为 IP:PORT
  - `--tun-name, -t`: TUN 设备名称，默认为 tun0
  - `--mtu, -m`: MTU 大小，默认为 1500

## 网络设置

程序不会自动配置网络接口，您需要手动配置。以下是一些常见的配置示例：

### 服务端配置

```bash
# 创建 TUN 设备后，配置服务端 IP 地址
sudo ip addr add 10.0.0.1/24 dev tun0
sudo ip link set tun0 up

# 如需开启 IP 转发
sudo sysctl -w net.ipv4.ip_forward=1

# 如需配置 NAT（用于连接外部网络）
sudo iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -j MASQUERADE
```

### 客户端配置

```bash
# 创建 TUN 设备后，配置客户端 IP 地址
sudo ip addr add 10.0.0.2/24 dev tun0  # 每个客户端使用不同的 IP
sudo ip link set tun0 up

# 如需通过服务器访问其他网络，添加路由
sudo ip route add 192.168.1.0/24 via 10.0.0.1
```

## 故障排除

- 如果无法建立连接，请检查防火墙设置，确保服务器UDP端口已开放。
- 如果网络通信有问题，请检查路由配置和 IP 设置是否正确。
- 由于使用UDP协议，端口映射需确保UDP流量可正常通过。
- 尝试使用更高的日志级别（如 `--log-level debug`）查看详细的调试信息。

## 许可证

MIT 