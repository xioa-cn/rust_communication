//! 设备名称、数制及访问范围校验；采用手册定义，不把 MC 数制套用到 A1E 编码。

use super::melsec_packet::Protocol;

/// 已解析的设备地址；一个地址单位是一个位设备点或一个字寄存器。
#[derive(Clone, Copy, Debug)]
pub(super) struct Address {
    /// 用于 ASCII 设备代码和错误信息的规范名称。
    pub name: &'static str,
    /// MC 二进制设备代码，iQ-R 扩展为两字节。
    pub code: u16,
    /// 设备起始编号，不是字节偏移。
    pub number: u32,
    /// 位设备可以按位或按字访问，按字时一步前移 16 点。
    pub bit_device: bool,
    /// MC ASCII 中设备编号使用的数制。
    pub radix: u32,
}

impl Address {
    /// 最长设备前缀优先匹配，并在连接之前拒绝非法符号和地址溢出。
    pub fn parse(text: &str, protocol: Protocol) -> Result<Self, String> {
        let text = text.trim().to_ascii_uppercase();
        let definitions = [
            ("STS", 0xc7, true, 10),
            ("STC", 0xc6, true, 10),
            ("STN", 0xc8, false, 10),
            ("LTS", 0x51, true, 10),
            ("LTC", 0x50, true, 10),
            ("LCS", 0x55, true, 10),
            ("LCC", 0x54, true, 10),
            ("SM", 0x91, true, 10),
            ("SD", 0xa9, false, 10),
            ("SB", 0xa1, true, 16),
            ("SW", 0xb5, false, 16),
            ("DX", 0xa2, true, 16),
            ("DY", 0xa3, true, 16),
            ("ZR", 0xb0, false, 16),
            ("RD", 0x2c, false, 10),
            ("TS", 0xc1, true, 10),
            ("TC", 0xc0, true, 10),
            ("TN", 0xc2, false, 10),
            ("CS", 0xc4, true, 10),
            ("CC", 0xc3, true, 10),
            ("CN", 0xc5, false, 10),
            ("SS", 0xc7, true, 10),
            ("SC", 0xc6, true, 10),
            ("SN", 0xc8, false, 10),
            ("M", 0x90, true, 10),
            ("X", 0x9c, true, 16),
            ("Y", 0x9d, true, 16),
            ("L", 0x92, true, 10),
            ("F", 0x93, true, 10),
            ("V", 0x94, true, 10),
            ("B", 0xa0, true, 16),
            ("D", 0xa8, false, 10),
            ("W", 0xb4, false, 16),
            ("R", 0xaf, false, 10),
            ("Z", 0xcc, false, 10),
            ("S", 0x98, true, 10),
        ];
        let (mut name, code, bit_device, radix) = definitions
            .into_iter()
            .find(|(name, _, _, _)| text.starts_with(name))
            .ok_or("Unsupported MELSEC device address")?;
        let digits = &text[name.len()..];
        if digits.is_empty()
            || !digits.bytes().all(|byte| match radix {
                16 => byte.is_ascii_hexdigit(),
                _ => byte.is_ascii_digit(),
            })
        {
            return Err("Invalid MELSEC device number; bit suffixes are not supported".into());
        }
        let number = u32::from_str_radix(digits, radix).map_err(|_| "MELSEC address overflow")?;
        name = match name {
            "STS" => "SS",
            "STC" => "SC",
            "STN" => "SN",
            other => other,
        };
        let address = Self {
            name,
            code,
            number,
            bit_device,
            radix,
        };
        if matches!(name, "LTS" | "LTC" | "LCS" | "LCC" | "RD") && !protocol.is_r() {
            return Err("This device requires MC R binary".into());
        }
        if protocol.is_a1e() {
            address.a1e_code()?;
        }
        address.validate(1, true, protocol)?;
        Ok(address)
    }

    /// A1E 使用双字符 ASCII 数值代码，不复用 MC 的单字节设备代码。
    pub fn a1e_code(self) -> Result<u16, String> {
        match self.name {
            "X" => Ok(0x5820),
            "Y" => Ok(0x5920),
            "M" => Ok(0x4d20),
            "F" => Ok(0x4620),
            "B" => Ok(0x4220),
            "D" => Ok(0x4420),
            "W" => Ok(0x5720),
            "R" => Ok(0x5220),
            "TN" => Ok(0x544e),
            "TS" => Ok(0x5453),
            "TC" => Ok(0x5443),
            "CN" => Ok(0x434e),
            "CS" => Ok(0x4353),
            "CC" => Ok(0x4343),
            _ => Err(format!(
                "{} is not supported by A1E batch access",
                self.name
            )),
        }
    }

    /// 验证整个操作范围；A1E 位设备的字访问要求 16 点对齐。
    pub fn validate(self, points: usize, bit: bool, protocol: Protocol) -> Result<(), String> {
        let span = if !bit && self.bit_device {
            points.checked_mul(16)
        } else {
            Some(points)
        }
        .filter(|span| *span > 0)
        .ok_or("MELSEC length is zero or overflows")?;
        let last = u64::from(self.number)
            .checked_add(u64::try_from(span - 1).map_err(|_| "MELSEC length overflow")?)
            .ok_or("MELSEC address range overflow")?;
        let maximum = if protocol.is_a1e() || protocol.is_r() {
            u64::from(u32::MAX)
        } else if protocol.is_ascii() && self.radix == 10 {
            999_999
        } else {
            0xff_ffff
        };
        if last > maximum {
            return Err("MELSEC operation exceeds the protocol address range".into());
        }
        if !bit && matches!(self.name, "LTS" | "LTC") {
            return Err("Long timer contacts/coils require bool access".into());
        }
        if !bit && self.bit_device && protocol.is_a1e() {
            let base = if protocol.is_a1e() && self.name == "M" && self.number >= 9000 {
                9000
            } else {
                0
            };
            if !(self.number - base).is_multiple_of(16) {
                return Err("Word access to a bit device requires a 16-bit aligned address".into());
            }
        }
        Ok(())
    }

    /// 按已验证的块点数推进地址；只有未完成的下一块才调用。
    pub fn advanced(self, points: usize, bit: bool) -> Self {
        let step = if !bit && self.bit_device { 16 } else { 1 };
        Self {
            number: (u64::from(self.number) + points as u64 * step) as u32,
            ..self
        }
    }
}
