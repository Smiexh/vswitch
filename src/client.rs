use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{self, Duration};
use std::io::Cursor;
use crate::error::{Result, VswitchError};
use crate::protocol::{Message, MessageType};
use crate::tun::TunDevice;

/// 客户端结构
pub struct Client {
    tun: Arc<TunDevice>,
    server_addr: SocketAddr,
}

impl Client {
    /// 创建一个新的客户端实例
    pub fn new(tun: TunDevice, server_addr: SocketAddr) -> Self {
        Self {
            tun: Arc::new(tun),
            server_addr,
        }
    }

    /// 启动客户端
    pub async fn run(&self) -> Result<()> {
        log::info!("客户端启动，连接服务器: {}", self.server_addr);
        
        // 创建UDP套接字
        let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
            log::error!("绑定UDP套接字失败: {}", e);
            VswitchError::IoError(e)
        })?;
        
        // 连接到服务器地址
        socket.connect(self.server_addr).await.map_err(|e| {
            log::error!("连接服务器失败: {}", e);
            VswitchError::IoError(e)
        })?;
        
        let local_addr = socket.local_addr().map_err(|e| {
            log::error!("获取本地地址失败: {}", e);
            VswitchError::IoError(e)
        })?;
        
        log::info!("UDP套接字绑定成功，本地地址: {}", local_addr);
        
        let socket = Arc::new(socket);
        
        // 发送连接消息
        log::info!("向服务器 {} 发送连接请求", self.server_addr);
        socket.send(&Message::connect().encode()).await.map_err(|e| {
            log::error!("发送连接消息失败: {}", e);
            VswitchError::IoError(e)
        })?;
        
        // 启动心跳任务
        let heartbeat_socket = socket.clone();
        self.spawn_heartbeat_task(heartbeat_socket);
        
        // 启动从TUN设备读取数据的任务
        let tun_reader_socket = socket.clone();
        self.spawn_tun_reader_task(tun_reader_socket);
        
        // 主循环：处理从服务器接收到的数据
        let mut recv_buf = vec![0u8; 4096];
        
        log::info!("客户端主循环开始运行，等待服务器数据");
        
        loop {
            match socket.recv(&mut recv_buf).await {
                Ok(size) => {
                    if size == 0 {
                        log::debug!("收到空数据包");
                        continue;
                    }
                    
                    let received_data = &recv_buf[..size];
                    let mut cursor = Cursor::new(received_data);
                    
                    match Message::decode(&mut cursor) {
                        Ok(message) => {
                            match message.msg_type {
                                MessageType::Connect => {
                                    log::info!("收到服务器连接确认");
                                }
                                MessageType::Data => {
                                    let payload_len = message.payload.len();
                                    log::debug!("从服务器接收数据包，长度: {} bytes", payload_len);
                                    
                                    // 写入TUN设备
                                    if let Err(e) = self.tun.write_packet(&message.payload).await {
                                        log::error!("写入TUN设备错误: {}, 数据包大小: {}", e, payload_len);
                                    } else {
                                        log::debug!("数据包成功写入TUN设备 ({} bytes)", payload_len);
                                    }
                                }
                                MessageType::Heartbeat => {
                                    log::debug!("收到服务器心跳响应");
                                }
                                MessageType::Disconnect => {
                                    log::info!("服务器请求断开连接");
                                    return Ok(());
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("解码消息错误: {}, 收到 {} bytes", e, size);
                        }
                    }
                }
                Err(e) => {
                    log::error!("从服务器接收数据错误: {}", e);
                    time::sleep(Duration::from_secs(1)).await;
                    
                    // 尝试重新连接服务器
                    log::info!("尝试重新连接服务器 {}...", self.server_addr);
                    if let Err(err) = socket.connect(self.server_addr).await {
                        log::error!("重新连接服务器失败: {}", err);
                    } else {
                        // 重新发送连接消息
                        log::info!("重新连接服务器成功，发送连接消息");
                        if let Err(err) = socket.send(&Message::connect().encode()).await {
                            log::error!("发送连接消息失败: {}", err);
                        } else {
                            log::info!("连接消息发送成功");
                        }
                    }
                }
            }
        }
    }

    /// 启动心跳任务
    /// 
    /// 该任务负责定期向服务器发送心跳消息，确保连接保持活跃
    fn spawn_heartbeat_task(&self, socket: Arc<UdpSocket>) {
        log::info!("启动心跳任务，每10秒发送一次心跳");
        
        tokio::spawn(async move {
            let heartbeat_interval = Duration::from_secs(10);
            
            loop {
                time::sleep(heartbeat_interval).await;
                
                let heartbeat = Message::heartbeat().encode();
                match socket.send(&heartbeat).await {
                    Ok(_) => {
                        log::debug!("心跳发送成功");
                    }
                    Err(e) => {
                        log::error!("发送心跳错误: {}, 心跳任务终止", e);
                        break;
                    }
                }
            }
            
            log::warn!("心跳任务已退出");
        });
    }

    /// 启动从TUN设备读取并发送到服务器的任务
    /// 
    /// 该任务负责从TUN设备读取数据包并转发到服务器
    fn spawn_tun_reader_task(&self, socket: Arc<UdpSocket>) {
        let tun = self.tun.clone();
        
        log::info!("启动TUN设备读取任务");
        
        tokio::spawn(async move {
            loop {
                match tun.read_packet().await {
                    Ok(packet) => {
                        let packet_len = packet.len();
                        log::debug!("从TUN设备读取数据包，长度: {} bytes", packet_len);
                        
                        let message = Message::data(packet);
                        let encoded = message.encode();
                        
                        match socket.send(&encoded).await {
                            Ok(_) => {
                                log::debug!("成功向服务器发送数据包 ({} bytes)", packet_len);
                            }
                            Err(e) => {
                                log::error!("向服务器发送数据错误: {}", e);
                                time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("从TUN设备读取错误: {}", e);
                        time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });
    }
} 