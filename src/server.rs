use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use std::io::Cursor;
use crate::error::{Result, VswitchError};
use crate::protocol::{Message, MessageType};
use crate::tun::TunDevice;
use bytes;

/// 表示一个已连接的客户端
struct Client {
    last_heartbeat: u64,
    /// 客户端的虚拟IP地址
    ip_addr: Option<IpAddr>,
}

impl Client {
    fn new(_addr: SocketAddr) -> Self {
        Self {
            last_heartbeat: current_time_millis(),
            ip_addr: None,
        }
    }
}

/// 服务端结构
pub struct Server {
    tun: Arc<TunDevice>,
    /// 客户端连接映射表 (UDP地址 -> 客户端信息)
    clients: Arc<Mutex<HashMap<SocketAddr, Client>>>,
    /// IP地址映射表 (IP地址 -> UDP地址)
    ip_to_addr: Arc<Mutex<HashMap<IpAddr, SocketAddr>>>,
}

impl Server {
    /// 创建一个新的服务端实例
    pub fn new(tun: TunDevice) -> Self {
        Self {
            tun: Arc::new(tun),
            clients: Arc::new(Mutex::new(HashMap::new())),
            ip_to_addr: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 启动服务端
    pub async fn run(&self, listen_addr: SocketAddr) -> Result<()> {
        log::info!("服务端启动，监听地址: {}", listen_addr);
        
        // 创建UDP套接字
        let socket = UdpSocket::bind(listen_addr).await.map_err(|e| {
            log::error!("绑定UDP套接字失败 {}: {}", listen_addr, e);
            VswitchError::IoError(e)
        })?;
        
        log::info!("UDP套接字绑定成功: {}", listen_addr);
        let socket = Arc::new(socket);
        
        // 启动TUN设备读取处理任务
        self.spawn_tun_reader(socket.clone());
        
        // 启动心跳检测任务
        self.spawn_heartbeat_checker();
        
        // 创建接收缓冲区
        let mut recv_buf = vec![0u8; 4096];
        
        log::info!("服务端主循环开始运行");
        
        // 主循环：处理客户端请求
        loop {
            match socket.recv_from(&mut recv_buf).await {
                Ok((size, addr)) => {
                    if size == 0 {
                        log::debug!("收到空数据包，来源: {}", addr);
                        continue;
                    }
                    
                    let received_data = &recv_buf[..size];
                    let mut cursor = Cursor::new(received_data);
                    
                    match Message::decode(&mut cursor) {
                        Ok(message) => {
                            match message.msg_type {
                                MessageType::Connect => {
                                    log::info!("客户端连接请求: {}", addr);
                                    
                                    // 添加或更新客户端
                                    let mut clients = self.clients.lock().await;
                                    let is_new_client = !clients.contains_key(&addr);
                                    if is_new_client {
                                        clients.insert(addr, Client::new(addr));
                                        log::info!("新客户端连接成功: {}, 当前客户端总数: {}", addr, clients.len());
                                    } else {
                                        log::info!("客户端重新连接: {}", addr);
                                    }
                                    
                                    // 发送连接确认
                                    if let Err(e) = socket.send_to(&Message::connect().encode(), addr).await {
                                        log::error!("发送连接确认错误 -> {}: {}", addr, e);
                                    } else {
                                        log::debug!("发送连接确认成功 -> {}", addr);
                                    }
                                }
                                MessageType::Data => {
                                    log::debug!("收到数据包: {} bytes from {}", message.payload.len(), addr);
                                    
                                    // 更新心跳时间
                                    self.update_client_heartbeat(addr).await;
                                    
                                    // 提取数据包源IP地址并更新映射表
                                    if let Some(src_ip) = extract_src_ip(&message.payload) {
                                        self.update_ip_mapping(addr, src_ip).await;
                                    }
                                    
                                    // 将数据写入TUN设备
                                    if let Err(e) = self.tun.write_packet(&message.payload).await {
                                        log::error!("写入TUN设备错误: {} (数据来源: {})", e, addr);
                                    } else {
                                        log::debug!("数据包成功写入TUN设备 ({} bytes)", message.payload.len());
                                    }
                                }
                                MessageType::Heartbeat => {
                                    log::debug!("收到心跳包: {}", addr);
                                    
                                    // 更新客户端心跳时间
                                    self.update_client_heartbeat(addr).await;
                                    
                                    // 发送心跳响应
                                    if let Err(e) = socket.send_to(&Message::heartbeat().encode(), addr).await {
                                        log::error!("发送心跳响应错误 -> {}: {}", addr, e);
                                    }
                                }
                                MessageType::Disconnect => {
                                    log::info!("客户端主动断开连接请求: {}", addr);
                                    self.remove_client(addr).await;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("解码消息错误: {} from {}, 数据大小: {}", e, addr, size);
                        }
                    }
                }
                Err(e) => {
                    log::error!("UDP接收错误: {}", e);
                    time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
    
    /// 更新客户端的最后心跳时间
    async fn update_client_heartbeat(&self, addr: SocketAddr) {
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get_mut(&addr) {
            client.last_heartbeat = current_time_millis();
            log::debug!("更新客户端心跳: {}", addr);
        } else {
            // 如果客户端不存在，则添加它
            clients.insert(addr, Client::new(addr));
            log::info!("通过活动数据添加新客户端: {}, 当前客户端总数: {}", addr, clients.len());
        }
    }
    
    /// 移除客户端及其IP映射
    async fn remove_client(&self, addr: SocketAddr) {
        // 移除客户端
        let mut client_ip = None;
        {
            let mut clients = self.clients.lock().await;
            if let Some(client) = clients.remove(&addr) {
                client_ip = client.ip_addr;
                log::info!("客户端已移除: {}, 剩余客户端: {}", addr, clients.len());
            } else {
                log::warn!("移除不存在的客户端: {}", addr);
            }
        }
        
        // 移除IP映射
        if let Some(ip) = client_ip {
            let mut ip_map = self.ip_to_addr.lock().await;
            if ip_map.remove(&ip).is_some() {
                log::info!("移除IP映射: {} -> {}", ip, addr);
            }
        }
    }
    
    /// 更新IP地址与客户端地址的映射关系
    async fn update_ip_mapping(&self, addr: SocketAddr, ip: IpAddr) {
        // 更新客户端的IP地址
        {
            let mut clients = self.clients.lock().await;
            if let Some(client) = clients.get_mut(&addr) {
                if client.ip_addr != Some(ip) {
                    log::info!("客户端 {} 的IP地址更新为: {}", addr, ip);
                    client.ip_addr = Some(ip);
                }
            }
        }
        
        // 更新IP到地址的映射
        let mut ip_map = self.ip_to_addr.lock().await;
        if let Some(old_addr) = ip_map.get(&ip) {
            if *old_addr != addr {
                log::warn!("IP地址 {} 从 {} 移动到 {}", ip, old_addr, addr);
            }
        }
        ip_map.insert(ip, addr);
    }

    /// 启动TUN设备读取任务
    fn spawn_tun_reader(&self, socket: Arc<UdpSocket>) {
        let tun = self.tun.clone();
        let ip_to_addr = self.ip_to_addr.clone();
        
        log::info!("启动TUN设备读取任务");
        
        tokio::spawn(async move {
            loop {
                match tun.read_packet().await {
                    Ok(packet) => {
                        let packet_len = packet.len();
                        log::debug!("从TUN设备读取数据包, 长度: {}", packet_len);
                        
                        // 创建数据消息
                        let message = Message::data(packet.clone());
                        let encoded = message.encode();
                        
                        // 确定目标客户端
                        let ip_map = ip_to_addr.lock().await;
                        
                        // 提取目标IP
                        match extract_dst_ip(&packet) {
                            Some(dst_ip) => {
                                // 查找目标IP对应的客户端地址
                                if let Some(dst_addr) = ip_map.get(&dst_ip) {
                                    // 向特定客户端发送数据
                                    log::debug!("向客户端 {} (IP: {}) 发送数据包, 长度: {}", dst_addr, dst_ip, packet_len);
                                    if let Err(e) = socket.send_to(&encoded, dst_addr).await {
                                        log::error!("向客户端 {} 发送数据错误: {}", dst_addr, e);
                                    }
                                } else {
                                    log::debug!("未找到目标IP对应的客户端: {}, 数据包被丢弃", dst_ip);
                                }
                            }
                            None => {
                                log::debug!("无法从数据包解析目标IP, 数据包被丢弃");
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

    /// 启动心跳检测任务
    fn spawn_heartbeat_checker(&self) {
        let clients = self.clients.clone();
        let ip_to_addr = self.ip_to_addr.clone();
        
        log::info!("启动客户端心跳检测任务");
        
        tokio::spawn(async move {
            let heartbeat_interval = Duration::from_secs(10);
            let heartbeat_timeout = 30000; // 30秒超时
            
            loop {
                // 等待检查间隔
                time::sleep(heartbeat_interval).await;
                let now = current_time_millis();
                
                let mut clients_to_remove = Vec::new();
                let mut ips_to_remove = Vec::new();
                
                // 识别超时的客户端
                {
                    let clients_guard = clients.lock().await;
                    
                    for (addr, client) in clients_guard.iter() {
                        // 如果超过超时时间没有心跳，认为客户端离线
                        let time_since_last_heartbeat = now - client.last_heartbeat;
                        if time_since_last_heartbeat > heartbeat_timeout && client.last_heartbeat > 0 {
                            log::info!("客户端 {} 心跳超时 ({} ms)", addr, time_since_last_heartbeat);
                            clients_to_remove.push(*addr);
                            if let Some(ip) = client.ip_addr {
                                ips_to_remove.push(ip);
                            }
                        }
                    }
                }
                
                // 移除超时的客户端
                if !clients_to_remove.is_empty() {
                    let mut clients_guard = clients.lock().await;
                    let mut ip_map = ip_to_addr.lock().await;
                    
                    for addr in &clients_to_remove {
                        clients_guard.remove(addr);
                        log::info!("移除超时客户端: {}, 剩余客户端: {}", addr, clients_guard.len());
                    }
                    
                    for ip in &ips_to_remove {
                        ip_map.remove(ip);
                        log::info!("移除IP映射: {}", ip);
                    }
                    
                    log::info!("心跳检测: 移除了 {} 个离线客户端", clients_to_remove.len());
                }
            }
        });
    }
}

/// 获取当前时间戳（毫秒）
fn current_time_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("时间错误")
        .as_millis() as u64
}

/// 提取IP数据包的源IP地址（全局函数）
fn extract_src_ip(packet: &bytes::Bytes) -> Option<IpAddr> {
    // 检查是否为IPv4数据包
    if packet.len() >= 20 && (packet[0] >> 4) == 4 {
        // IPv4: 源地址从12字节开始，长度4字节
        let src_ip = std::net::Ipv4Addr::new(
            packet[12], packet[13], packet[14], packet[15]
        );
        return Some(IpAddr::V4(src_ip));
    } 
    // 检查是否为IPv6数据包
    else if packet.len() >= 40 && (packet[0] >> 4) == 6 {
        // IPv6: 源地址从8字节开始，长度16字节
        let mut src_ip_bytes = [0u8; 16];
        src_ip_bytes.copy_from_slice(&packet[8..24]);
        let src_ip = std::net::Ipv6Addr::from(src_ip_bytes);
        return Some(IpAddr::V6(src_ip));
    }
    
    None
}

/// 提取IP数据包的目标IP地址（全局函数）
fn extract_dst_ip(packet: &bytes::Bytes) -> Option<IpAddr> {
    // 检查是否为IPv4数据包
    if packet.len() >= 20 && (packet[0] >> 4) == 4 {
        // IPv4: 目标地址从16字节开始，长度4字节
        let dst_ip = std::net::Ipv4Addr::new(
            packet[16], packet[17], packet[18], packet[19]
        );
        return Some(IpAddr::V4(dst_ip));
    } 
    // 检查是否为IPv6数据包
    else if packet.len() >= 40 && (packet[0] >> 4) == 6 {
        // IPv6: 目标地址从24字节开始，长度16字节
        let mut dst_ip_bytes = [0u8; 16];
        dst_ip_bytes.copy_from_slice(&packet[24..40]);
        let dst_ip = std::net::Ipv6Addr::from(dst_ip_bytes);
        return Some(IpAddr::V6(dst_ip));
    }
    
    None
} 