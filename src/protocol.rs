use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{Cursor, Read};
use crate::error::{Result, VswitchError};

/// 消息类型枚举
///
/// 定义了虚拟交换机协议支持的所有消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// 连接请求/确认消息
    Connect = 0x01,
    /// 数据传输消息
    Data = 0x02,
    /// 心跳消息
    Heartbeat = 0x03,
    /// 断开连接消息
    Disconnect = 0x04,
}

impl TryFrom<u8> for MessageType {
    type Error = VswitchError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0x01 => Ok(MessageType::Connect),
            0x02 => Ok(MessageType::Data),
            0x03 => Ok(MessageType::Heartbeat),
            0x04 => Ok(MessageType::Disconnect),
            _ => Err(VswitchError::InvalidProtocolMessage(format!("未知的消息类型: {}", value))),
        }
    }
}

/// 协议消息结构
///
/// 消息格式:
/// +------------------+------------------+--------------------+
/// |  消息类型 (1字节)  |  消息长度 (4字节)  |  消息内容 (变长)    |
/// +------------------+------------------+--------------------+
#[derive(Debug, Clone)]
pub struct Message {
    /// 消息类型
    pub msg_type: MessageType,
    /// 消息负载
    pub payload: Bytes,
}

impl Message {
    /// 创建一个新的消息
    pub fn new(msg_type: MessageType, payload: Bytes) -> Self {
        Self { msg_type, payload }
    }

    /// 创建一个连接消息
    pub fn connect() -> Self {
        Self::new(MessageType::Connect, Bytes::new())
    }

    /// 创建一个数据消息
    pub fn data(payload: Bytes) -> Self {
        Self::new(MessageType::Data, payload)
    }

    /// 创建一个心跳消息
    pub fn heartbeat() -> Self {
        Self::new(MessageType::Heartbeat, Bytes::new())
    }

    /// 创建一个断开连接消息
    #[allow(dead_code)]
    pub fn disconnect() -> Self {
        Self::new(MessageType::Disconnect, Bytes::new())
    }

    /// 将消息编码为字节序列
    ///
    /// 返回的字节序列格式:
    /// - 1字节: 消息类型
    /// - 4字节: 负载长度 (网络字节序)
    /// - N字节: 负载内容
    pub fn encode(&self) -> Bytes {
        let payload_len = self.payload.len();
        let mut buf = BytesMut::with_capacity(5 + payload_len);
        
        buf.put_u8(self.msg_type as u8);
        buf.put_u32(payload_len as u32);
        buf.put_slice(&self.payload);
        
        buf.freeze()
    }

    /// 从字节流解码消息
    ///
    /// 参数:
    /// - `buf`: 包含消息数据的字节缓冲区游标
    ///
    /// 返回:
    /// - 成功: 解码后的消息
    /// - 错误: 解码过程中的错误
    pub fn decode(buf: &mut Cursor<&[u8]>) -> Result<Self> {
        // 确保缓冲区至少包含消息头(类型+长度)
        if buf.remaining() < 5 {
            return Err(VswitchError::InvalidProtocolMessage("消息太短".to_string()));
        }

        // 读取消息类型
        let msg_type = MessageType::try_from(buf.get_u8())?;
        
        // 读取负载长度
        let payload_len = buf.get_u32() as usize;

        // 确保缓冲区包含完整的负载
        if buf.remaining() < payload_len {
            return Err(VswitchError::InvalidProtocolMessage("消息内容不完整".to_string()));
        }

        // 读取负载内容
        let mut payload = vec![0; payload_len];
        buf.read_exact(&mut payload).map_err(|e| VswitchError::IoError(e))?;

        Ok(Self {
            msg_type,
            payload: Bytes::from(payload),
        })
    }
} 