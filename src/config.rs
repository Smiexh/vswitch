use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use crate::error::{Result, VswitchError};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Config {
    /// 日志级别 (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "info")]
    pub log_level: String,

    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Mode {
    /// 服务端模式
    Server {
        /// 监听地址
        #[arg(short, long, default_value = "0.0.0.0:4789")]
        listen: String,

        /// TUN设备名称
        #[arg(short, long, default_value = "tun0")]
        tun_name: String,

        /// TUN设备MTU
        #[arg(short, long, default_value = "1500")]
        mtu: usize,
    },

    /// 客户端模式
    Client {
        /// 服务器地址
        #[arg(short, long)]
        server: String,

        /// TUN设备名称
        #[arg(short, long, default_value = "tun0")]
        tun_name: String,

        /// TUN设备MTU
        #[arg(short, long, default_value = "1500")]
        mtu: usize,
    },
}

impl Config {
    pub fn parse_args() -> Self {
        Config::parse()
    }

    pub fn get_server_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            Mode::Client { server, .. } => {
                server.parse().map_err(|e| VswitchError::ConfigError(format!("无效的服务器地址: {}", e)))
            }
            _ => Err(VswitchError::ConfigError("不是客户端模式".to_string())),
        }
    }

    pub fn get_listen_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            Mode::Server { listen, .. } => {
                listen.parse().map_err(|e| VswitchError::ConfigError(format!("无效的监听地址: {}", e)))
            }
            _ => Err(VswitchError::ConfigError("不是服务端模式".to_string())),
        }
    }

    #[allow(dead_code)]
    pub fn get_tun_name(&self) -> &str {
        match &self.mode {
            Mode::Server { tun_name, .. } => tun_name,
            Mode::Client { tun_name, .. } => tun_name,
        }
    }

    #[allow(dead_code)]
    pub fn get_mtu(&self) -> usize {
        match &self.mode {
            Mode::Server { mtu, .. } => *mtu,
            Mode::Client { mtu, .. } => *mtu,
        }
    }
} 