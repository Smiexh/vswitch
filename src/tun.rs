use tun::platform::posix::{Reader, Writer};
use tokio::sync::Mutex;
use std::sync::Arc;
use bytes::Bytes;
use std::io::{Read, Write};
use crate::error::{Result, VswitchError};

/// TUN设备结构
/// 
/// 封装TUN设备的读写操作，提供线程安全的接口
pub struct TunDevice {
    /// 设备读取器
    reader: Arc<Mutex<Reader>>,
    /// 设备写入器
    writer: Arc<Mutex<Writer>>,
    /// TUN设备名称
    name: String,
}

impl TunDevice {
    /// 创建一个新的TUN设备实例
    /// 
    /// 参数:
    /// - `name`: TUN设备名称
    /// - `mtu`: 最大传输单元大小
    pub fn new(name: &str, mtu: usize) -> Result<Self> {
        log::info!("正在创建TUN设备: {}, MTU: {}", name, mtu);
        
        // 配置TUN设备
        let mut config = tun::Configuration::default();
        config.name(name)
            .mtu(mtu as i32)
            .up();
        
        // 创建TUN设备
        let device = tun::create(&config).map_err(|e| {
            log::error!("创建TUN设备失败: {}", e);
            VswitchError::TunError(e)
        })?;
        
        // 分离读写器
        let (reader, writer) = device.split();
        
        log::info!("TUN设备 {} 创建成功", name);
        
        Ok(Self {
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            name: name.to_string(),
        })
    }

    /// 获取TUN设备名称
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 从TUN设备读取数据包
    /// 
    /// 返回:
    /// - 成功: 包含数据包内容的Bytes
    /// - 错误: 读取过程中的错误
    pub async fn read_packet(&self) -> Result<Bytes> {
        // 锁定读取器
        let mut reader = self.reader.lock().await;
        
        // 读取数据包
        let mut buf = vec![0u8; 2048]; // 使用较大的缓冲区以适应各种MTU
        let size = reader.read(&mut buf).map_err(|e| {
            log::error!("从TUN设备 {} 读取失败: {}", self.name, e);
            VswitchError::IoError(e)
        })?;
        
        buf.truncate(size);
        
        log::trace!("从TUN设备 {} 读取了 {} 字节", self.name, size);
        Ok(Bytes::from(buf))
    }

    /// 向TUN设备写入数据包
    /// 
    /// 参数:
    /// - `packet`: 要写入的数据包
    /// 
    /// 返回:
    /// - 成功: 成功写入的字节数
    /// - 错误: 写入过程中的错误
    pub async fn write_packet(&self, packet: &Bytes) -> Result<usize> {
        // 锁定写入器
        let mut writer = self.writer.lock().await;
        
        // 写入数据包
        let size = writer.write(packet).map_err(|e| {
            log::error!("写入TUN设备 {} 失败: {}", self.name, e);
            VswitchError::IoError(e)
        })?;
        
        log::trace!("向TUN设备 {} 写入了 {} 字节", self.name, size);
        Ok(size)
    }
}

/// 创建并返回TUN设备实例
/// 
/// 参数:
/// - `name`: TUN设备名称
/// - `mtu`: 最大传输单元大小
/// 
/// 返回:
/// - 成功: TUN设备实例
/// - 错误: 创建过程中的错误
pub fn create_tun_device(name: &str, mtu: u32) -> Result<TunDevice> {
    TunDevice::new(name, mtu as usize)
} 