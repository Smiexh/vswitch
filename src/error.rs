use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum VswitchError {
    #[error("IO错误: {0}")]
    IoError(#[from] io::Error),

    #[error("TUN设备错误: {0}")]
    TunError(#[from] tun::Error),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("无效的协议消息: {0}")]
    InvalidProtocolMessage(String),
}

pub type Result<T> = std::result::Result<T, VswitchError>; 