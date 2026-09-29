//! MC 3E、iQ-R 扩展设备指令及 A1E 的报文编解码，不依赖 Socket。

use super::melsec_address::Address;

/// 编码类型与设备指令格式；MC R 使用 3E 外层帧，不是 4E 序列号帧。
#[derive(Clone, Copy)]
pub(super) enum Protocol {
    McBinary,
    McAscii,
    McRBinary,
    A1EBinary,
    A1EAscii,
}

impl Protocol {
    /// ASCII 以字符数作为长度，不能按二进制字节数除以二。
    pub fn is_ascii(self) -> bool {
        matches!(self, Self::McAscii | Self::A1EAscii)
    }
    /// A1E 没有响应长度字段，必须结合命令和结束码收包。
    pub fn is_a1e(self) -> bool {
        matches!(self, Self::A1EBinary | Self::A1EAscii)
    }
    /// iQ-R 指令使用四字节设备号、两字节设备代码。
    pub fn is_r(self) -> bool {
        matches!(self, Self::McRBinary)
    }

    /// 根据帧格式、操作单位和设备种类计算单次批量访问上限。
    pub fn max_points(self, address: Address, bit: bool, write: bool) -> usize {
        if self.is_a1e() {
            if bit || !address.bit_device {
                256
            } else if write {
                40
            } else {
                128
            }
        } else if bit {
            if self.is_ascii() { 3584 } else { 7168 }
        } else {
            960
        }
    }

    /// 计算固定响应头长度；不包含正常响应数据。
    pub fn header_len(self) -> usize {
        match self {
            Self::A1EBinary => 2,
            Self::A1EAscii => 4,
            Self::McAscii => 18,
            _ => 9,
        }
    }

    /// 计算数据区的线上字节数；A1E ASCII 奇数位补一个占位字符。
    pub fn payload_len(self, bit: bool, points: usize) -> usize {
        if bit {
            if self.is_ascii() {
                if self.is_a1e() {
                    points.div_ceil(2) * 2
                } else {
                    points
                }
            } else {
                points.div_ceil(2)
            }
        } else {
            points * if self.is_ascii() { 4 } else { 2 }
        }
    }
}

/// 路由与 PLC 监视定时器；A1E 仅使用 PC 号及定时器。
#[derive(Clone, Copy)]
pub(super) struct Route {
    pub network: u8,
    pub pc: u8,
    pub io: u16,
    pub station: u8,
    pub timer: u16,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            network: 0,
            pc: 0xff,
            io: 0x03ff,
            station: 0,
            timer: 16,
        }
    }
}

/// 严格解码 ASCII 十六进制字段，不接受空白、符号或非 ASCII 字符。
pub(super) fn hex(bytes: &[u8]) -> Result<u16, String> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err("Invalid MELSEC ASCII hexadecimal field".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid MELSEC ASCII field")?;
    u16::from_str_radix(text, 16).map_err(|_| "MELSEC hexadecimal field overflow".into())
}

/// A1E 用一个字节区分字/位和读/写；响应命令增加 0x80。
pub(super) fn a1e_command(bit: bool, write: bool) -> u8 {
    u8::from(!bit) + 2 * u8::from(write)
}

/// 组装批量读写请求；内部位数据每字节一位，数值数据采用小端字节流。
pub(super) fn request(
    protocol: Protocol,
    route: Route,
    address: Address,
    bit: bool,
    points: usize,
    data: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let write = data.is_some();
    if points == 0 || points > protocol.max_points(address, bit, write) {
        return Err("MELSEC request exceeds point limit".into());
    }
    let mut payload = Vec::new();
    if protocol.is_a1e() {
        let command = a1e_command(bit, write);
        let code = address.a1e_code()?;
        // A1E 的点数只有一个字节，256 编为 00，紧随其后的固定字节也为 00。
        if protocol.is_ascii() {
            payload.extend_from_slice(
                format!(
                    "{command:02X}{:02X}{:04X}{code:04X}{:08X}{:02X}00",
                    route.pc, route.timer, address.number, points as u8
                )
                .as_bytes(),
            );
        } else {
            payload.extend_from_slice(&[command, route.pc]);
            payload.extend_from_slice(&route.timer.to_le_bytes());
            payload.extend_from_slice(&address.number.to_le_bytes());
            payload.extend_from_slice(&code.to_le_bytes());
            payload.extend_from_slice(&[points as u8, 0]);
        }
    } else {
        let command: u16 = if write { 0x1401 } else { 0x0401 };
        let subcommand = u16::from(bit) + if protocol.is_r() { 2 } else { 0 };
        if protocol.is_ascii() {
            let name = format!("{:*<2}", address.name);
            let number = if address.radix == 16 {
                format!("{:06X}", address.number)
            } else {
                format!("{:06}", address.number)
            };
            payload.extend_from_slice(
                format!(
                    "{:04X}{command:04X}{subcommand:04X}{name}{number}{points:04X}",
                    route.timer
                )
                .as_bytes(),
            );
        } else {
            payload.extend_from_slice(&route.timer.to_le_bytes());
            payload.extend_from_slice(&command.to_le_bytes());
            payload.extend_from_slice(&subcommand.to_le_bytes());
            if protocol.is_r() {
                payload.extend_from_slice(&address.number.to_le_bytes());
                payload.extend_from_slice(&address.code.to_le_bytes());
            } else {
                payload.extend_from_slice(&address.number.to_le_bytes()[..3]);
                payload.push(address.code as u8);
            }
            payload.extend_from_slice(&(points as u16).to_le_bytes());
        }
    }
    if let Some(data) = data {
        payload.extend_from_slice(&encode(protocol, bit, points, data)?);
    }
    if protocol.is_a1e() {
        return Ok(payload);
    }
    let length = u16::try_from(payload.len()).map_err(|_| "MELSEC request length overflow")?;
    let mut packet = if protocol.is_ascii() {
        format!(
            "5000{:02X}{:02X}{:04X}{:02X}{length:04X}",
            route.network, route.pc, route.io, route.station
        )
        .into_bytes()
    } else {
        let mut header = vec![0x50, 0, route.network, route.pc];
        header.extend_from_slice(&route.io.to_le_bytes());
        header.push(route.station);
        header.extend_from_slice(&length.to_le_bytes());
        header
    };
    packet.extend_from_slice(&payload);
    Ok(packet)
}

/// 数值按字转换 ASCII；二进制位数据每个字节包含高、低两个半字节。
fn encode(protocol: Protocol, bit: bool, points: usize, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() != if bit { points } else { points * 2 } {
        return Err("Invalid MELSEC write data length".into());
    }
    if bit {
        if data.iter().any(|value| *value > 1) {
            return Err("Invalid MELSEC boolean value".into());
        }
        if protocol.is_ascii() {
            let mut result: Vec<u8> = data.iter().map(|value| b'0' + value).collect();
            if protocol.is_a1e() && !points.is_multiple_of(2) {
                result.push(b'0');
            }
            Ok(result)
        } else {
            Ok(data
                .chunks(2)
                .map(|pair| (pair[0] << 4) | pair.get(1).copied().unwrap_or(0))
                .collect())
        }
    } else if protocol.is_ascii() {
        let mut result = Vec::with_capacity(points * 4);
        for word in data.as_chunks::<2>().0 {
            result.extend_from_slice(
                format!("{:04X}", u16::from_le_bytes([word[0], word[1]])).as_bytes(),
            );
        }
        Ok(result)
    } else {
        Ok(data.to_vec())
    }
}

/// 检查 MC 响应头的标志和路由，再返回后续数据长度。
pub(super) fn mc_body_len(
    protocol: Protocol,
    route: Route,
    header: &[u8],
) -> Result<usize, String> {
    if header.len() != protocol.header_len() {
        return Err("Invalid MELSEC response header length".into());
    }
    let length = if protocol.is_ascii() {
        if !header[..4].eq_ignore_ascii_case(b"D000")
            || hex(&header[4..6])? != u16::from(route.network)
            || hex(&header[6..8])? != u16::from(route.pc)
            || hex(&header[8..12])? != route.io
            || hex(&header[12..14])? != u16::from(route.station)
        {
            return Err("Invalid MELSEC response subheader or route".into());
        }
        usize::from(hex(&header[14..18])?)
    } else {
        if header[..2] != [0xd0, 0]
            || header[2] != route.network
            || header[3] != route.pc
            || u16::from_le_bytes([header[4], header[5]]) != route.io
            || header[6] != route.station
        {
            return Err("Invalid MELSEC response subheader or route".into());
        }
        usize::from(u16::from_le_bytes([header[7], header[8]]))
    };
    let minimum = if protocol.is_ascii() { 4 } else { 2 };
    if !(minimum..=8192).contains(&length) {
        return Err("Invalid MELSEC response body length".into());
    }
    Ok(length)
}

/// 先解析 A1E 结束码，防止错误短帧被当作正常数据一直等待。
pub(super) fn a1e_status(
    protocol: Protocol,
    header: &[u8],
    bit: bool,
    write: bool,
) -> Result<u16, String> {
    if header.len() != protocol.header_len() {
        return Err("Invalid A1E response header length".into());
    }
    let (command, status) = if protocol.is_ascii() {
        (hex(&header[..2])?, hex(&header[2..4])?)
    } else {
        (u16::from(header[0]), u16::from(header[1]))
    };
    if command != u16::from(a1e_command(bit, write) + 0x80) {
        return Err("A1E response command mismatch".into());
    }
    Ok(status)
}

/// 校验结束码及数据长度，将线上数据还原为统一的小端字节流或 0/1 位数组。
pub(super) fn response(
    protocol: Protocol,
    route: Route,
    packet: &[u8],
    bit: bool,
    points: usize,
    write: bool,
) -> Result<Vec<u8>, String> {
    let header_len = protocol.header_len();
    if packet.len() < header_len {
        return Err("Truncated MELSEC response".into());
    }
    let payload = if protocol.is_a1e() {
        let status = a1e_status(protocol, &packet[..header_len], bit, write)?;
        if status != 0 {
            return Err(format!(
                "A1E PLC end code 0x{status:02X}; details {:02X?}",
                &packet[header_len..]
            ));
        }
        &packet[header_len..]
    } else {
        let body_len = mc_body_len(protocol, route, &packet[..header_len])?;
        if packet.len() != header_len + body_len {
            return Err("MELSEC response length mismatch".into());
        }
        let body = &packet[header_len..];
        let (status, offset) = if protocol.is_ascii() {
            (hex(&body[..4])?, 4)
        } else {
            (u16::from_le_bytes([body[0], body[1]]), 2)
        };
        if status != 0 {
            return Err(format!("MELSEC PLC end code 0x{status:04X}"));
        }
        &body[offset..]
    };
    let expected = if write {
        0
    } else {
        protocol.payload_len(bit, points)
    };
    if payload.len() != expected {
        return Err(format!(
            "Invalid MELSEC payload length: expected {expected}, got {}",
            payload.len()
        ));
    }
    if write {
        return Ok(Vec::new());
    }
    if bit {
        let mut result = Vec::with_capacity(points);
        for index in 0..points {
            let value = if protocol.is_ascii() {
                payload[index]
                    .checked_sub(b'0')
                    .ok_or("Invalid MELSEC ASCII bit")?
            } else if index.is_multiple_of(2) {
                payload[index / 2] >> 4
            } else {
                payload[index / 2] & 0x0f
            };
            if value > 1 {
                return Err("Invalid MELSEC boolean payload".into());
            }
            result.push(value);
        }
        Ok(result)
    } else if protocol.is_ascii() {
        let mut result = Vec::with_capacity(points * 2);
        for word in payload.as_chunks::<4>().0 {
            result.extend_from_slice(&hex(word)?.to_le_bytes());
        }
        Ok(result)
    } else {
        Ok(payload.to_vec())
    }
}
