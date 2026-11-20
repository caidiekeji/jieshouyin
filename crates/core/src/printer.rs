//! 网络打印机输出（§7.5 IP:9100）。
//!
//! escpos 只负责渲染出打印字节流，最终要落到纸上。桌面收银机最常见的是
//! 网络小票打印机：TCP 直连 `IP:9100`，把字节流写进去即完成送纸。
//! 打印失败不阻断收银——调用方负责把订单号入重打队列。

use crate::error::{CoreError, Result};
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// 打印连接与写入超时（秒）：打印失败要快速返回，不能把收银台卡死。
const PRINT_TIMEOUT_SECS: u64 = 5;

/// 把打印字节流送到网络打印机。
/// host 为空表示未配置打印机，返回错误交由调用方入重打队列。
pub fn send_bytes(host: &str, port: u16, bytes: &[u8]) -> Result<()> {
    let h = host.trim();
    if h.is_empty() {
        return Err(CoreError::Validation("未配置打印机地址".into()));
    }
    let port = if port == 0 {
        crate::constants::DEFAULT_PRINT_PORT
    } else {
        port
    };
    let addr = (h, port)
        .to_socket_addrs()
        .map_err(|e| CoreError::Other(format!("打印机地址无法解析: {e}")))?
        .next()
        .ok_or_else(|| CoreError::Other(format!("打印机地址无效: {h}:{port}")))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(PRINT_TIMEOUT_SECS))
        .map_err(|e| CoreError::Other(format!("连接打印机失败: {e}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(PRINT_TIMEOUT_SECS)))
        .map_err(|e| CoreError::Other(format!("设置打印超时失败: {e}")))?;
    stream
        .write_all(bytes)
        .map_err(|e| CoreError::Other(format!("发送打印数据失败: {e}")))?;
    stream
        .flush()
        .map_err(|e| CoreError::Other(format!("打印数据发送未完成: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_host_rejected() {
        assert!(send_bytes("", 9100, b"x").is_err());
        assert!(send_bytes("   ", 9100, b"x").is_err());
    }
}
