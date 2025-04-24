mod config;
mod error;
mod protocol;
mod tun;
mod server;
mod client;

use crate::config::{Config, Mode};
use crate::error::Result;
use crate::tun::create_tun_device;
use crate::server::Server;
use crate::client::Client;

#[tokio::main]
async fn main() -> Result<()> {
    // 解析命令行参数
    let config = Config::parse_args();
    
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&config.log_level))
        .init();
    
    log::info!("虚拟交换机启动, 版本: 0.1.0");
    log::info!("日志级别: {}", config.log_level);
    
    // 根据模式创建TUN设备并启动服务
    match &config.mode {
        Mode::Server { listen: _, tun_name, mtu } => {
            log::info!("运行模式: 服务端");
            
            let listen_addr = config.get_listen_addr()?;
            
            log::info!("TUN设备名称: {}, MTU: {}, 监听地址: {}", tun_name, mtu, listen_addr);
            
            // 创建TUN设备
            log::info!("正在创建TUN设备...");
            let tun = create_tun_device(tun_name, *mtu as u32)?;
            log::info!("TUN设备创建成功: {}", tun.name());
            
            // 创建并启动服务端
            log::info!("正在初始化服务端...");
            let server = Server::new(tun);
            
            log::info!("服务端初始化完成，开始运行...");
            server.run(listen_addr).await?;
        }
        Mode::Client { server: _, tun_name, mtu } => {
            log::info!("运行模式: 客户端");
            
            let server_addr = config.get_server_addr()?;
            
            log::info!("TUN设备名称: {}, MTU: {}, 服务器地址: {}", tun_name, mtu, server_addr);
            
            // 创建TUN设备
            log::info!("正在创建TUN设备...");
            let tun = create_tun_device(tun_name, *mtu as u32)?;
            log::info!("TUN设备创建成功: {}", tun.name());
            
            // 创建并启动客户端
            log::info!("正在初始化客户端...");
            let client = Client::new(tun, server_addr);
            
            log::info!("客户端初始化完成，开始连接服务器: {}...", server_addr);
            client.run().await?;
        }
    }
    
    log::info!("虚拟交换机已退出");
    Ok(())
}
