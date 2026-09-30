//! RFC 1006 TPKT 收发及 COTP 数据分段重组。

use std::io::{Read, Write};
use std::net::TcpStream;

/// 添加 TPKT 头并完整发送；不能假设一次 write 就写出了全部数据。
pub(super) fn send_tpkt(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let length = u16::try_from(payload.len() + 4).map_err(|_| "TPKT is too large")?;
    let mut packet = Vec::with_capacity(usize::from(length));
    packet.extend_from_slice(&[3, 0]);
    packet.extend_from_slice(&length.to_be_bytes());
    packet.extend_from_slice(payload);
    stream.write_all(&packet).map_err(io_error)
}

/// 先读取固定 4 字节头，再按声明长度读取负载，处理 TCP 拆包与粘包。
/// 在分配内存前限制报文长度，避免异常报文触发大内存分配。
pub(super) fn receive_tpkt(stream: &mut impl Read, maximum: usize) -> Result<Vec<u8>, String> {
    let length = receive_length(stream, maximum)?;
    let mut payload = vec![0; length - 4];
    stream.read_exact(&mut payload).map_err(io_error)?;
    Ok(payload)
}

fn receive_length(stream: &mut impl Read, maximum: usize) -> Result<usize, String> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).map_err(io_error)?;
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if header[0..2] != [3, 0] || length < 7 || length > maximum {
        return Err("Invalid TPKT header or packet length".into());
    }
    Ok(length)
}

/// 按 COTP 的 EOT 标志重组一个完整 S7 PDU。
/// 限制累计长度并拒绝空分段，防止无限追加或永不结束的空包。
pub(super) fn receive_data(stream: &mut impl Read, pdu_length: usize) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    loop {
        let length = receive_length(stream, pdu_length + 7)?;
        let mut cotp = [0; 3];
        stream.read_exact(&mut cotp).map_err(io_error)?;
        if length == 7 || cotp[0..2] != [2, 0xf0] || cotp[2] & 0x7f != 0 {
            return Err("Invalid COTP Data packet".into());
        }
        let previous_length = response.len();
        let total_length = previous_length + length - 7;
        if total_length > pdu_length {
            return Err("Response exceeds negotiated S7 PDU length".into());
        }
        response.resize(total_length, 0);
        stream
            .read_exact(&mut response[previous_length..])
            .map_err(io_error)?;
        if cotp[2] & 0x80 != 0 {
            return Ok(response);
        }
    }
}

/// 为系统 Socket 错误添加通讯上下文。
pub(super) fn io_error(error: std::io::Error) -> String {
    format!("S7 socket error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    struct CountingReader {
        bytes: Cursor<Vec<u8>>,
        chunk: usize,
        reads: usize,
    }

    impl Read for CountingReader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            let count = output.len().min(self.chunk);
            self.bytes.read(&mut output[..count])
        }
    }

    fn frame(eot: bool, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![3, 0];
        bytes.extend_from_slice(&((payload.len() + 7) as u16).to_be_bytes());
        bytes.extend_from_slice(&[2, 0xf0, if eot { 0x80 } else { 0 }]);
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn buffered_receive_retains_coalesced_segments_and_following_pdu() {
        let bytes = [frame(false, b"ab"), frame(true, b"cd"), frame(true, b"ef")].concat();
        let mut reader = BufReader::new(CountingReader {
            bytes: Cursor::new(bytes),
            chunk: usize::MAX,
            reads: 0,
        });
        assert_eq!(receive_data(&mut reader, 4).unwrap(), b"abcd");
        assert_eq!(receive_data(&mut reader, 4).unwrap(), b"ef");
        assert_eq!(reader.get_ref().reads, 1);
    }

    #[test]
    fn buffered_receive_handles_every_small_fragment_size() {
        for chunk in 1..32 {
            let bytes = [frame(false, b"ab"), frame(true, b"cd"), frame(true, b"ef")].concat();
            let mut reader = BufReader::with_capacity(
                8,
                CountingReader {
                    bytes: Cursor::new(bytes),
                    chunk,
                    reads: 0,
                },
            );
            assert_eq!(receive_data(&mut reader, 4).unwrap(), b"abcd");
            assert_eq!(receive_data(&mut reader, 4).unwrap(), b"ef");
        }
    }

    #[test]
    fn direct_assembly_rejects_empty_oversized_and_unfinished_segments() {
        let mut bad_cotp = frame(true, b"a");
        bad_cotp[6] = 0x81;
        for bytes in [
            frame(true, b""),
            frame(false, b"ab"),
            frame(true, b"abcde"),
            [frame(false, b"abc"), frame(true, b"de")].concat(),
            bad_cotp,
            vec![3, 0, 0xff, 0xff],
            vec![3, 0, 0, 6],
        ] {
            assert!(receive_data(&mut Cursor::new(bytes), 4).is_err());
        }
    }
}
