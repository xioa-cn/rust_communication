//! RFC 1006 TPKT 收发及 COTP 数据分段重组。

use std::io::{Read, Write};
use std::net::TcpStream;

/// 添加 TPKT 头并完整发送；不能假设一次 write 就写出了全部数据。
pub(super) fn send_tpkt(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let length = u16::try_from(payload.len() + 4).map_err(|_| "TPKT is too large")?;
    let mut packet = vec![3, 0];
    packet.extend_from_slice(&length.to_be_bytes());
    packet.extend_from_slice(payload);
    stream.write_all(&packet).map_err(io_error)
}

/// 先读取固定 4 字节头，再按声明长度读取负载，处理 TCP 拆包与粘包。
/// 在分配内存前限制报文长度，避免异常报文触发大内存分配。
pub(super) fn receive_tpkt(stream: &mut TcpStream, maximum: usize) -> Result<Vec<u8>, String> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).map_err(io_error)?;
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if header[0..2] != [3, 0] || length < 7 || length > maximum {
        return Err("Invalid TPKT header or packet length".into());
    }
    let mut payload = vec![0; length - 4];
    stream.read_exact(&mut payload).map_err(io_error)?;
    Ok(payload)
}

/// 按 COTP 的 EOT 标志重组一个完整 S7 PDU。
/// 限制累计长度并拒绝空分段，防止无限追加或永不结束的空包。
pub(super) fn receive_data(stream: &mut TcpStream, pdu_length: usize) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    loop {
        let packet = receive_tpkt(stream, pdu_length + 7)?;
        if packet.len() <= 3 || packet[0..2] != [2, 0xf0] || packet[2] & 0x7f != 0 {
            return Err("Invalid COTP Data packet".into());
        }
        if response.len() + packet.len() - 3 > pdu_length {
            return Err("Response exceeds negotiated S7 PDU length".into());
        }
        response.extend_from_slice(&packet[3..]);
        if packet[2] & 0x80 != 0 {
            return Ok(response);
        }
    }
}

/// 为系统 Socket 错误添加通讯上下文。
pub(super) fn io_error(error: std::io::Error) -> String {
    format!("S7 socket error: {error}")
}
