use super::InovanceType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Coil,
    DiscreteInput,
    Word,
    WordBit(u8),
    Byte(u8),
}

#[derive(Clone, Debug)]
pub(super) struct Address {
    pub kind: Kind,
    pub offset: u16,
    pub remaining: usize,
    pub unit: Option<u8>,
    pub extended: bool,
    pub read_only: bool,
    pub counter32: bool,
}

impl Address {
    pub fn parse(text: &str, series: InovanceType, bit: bool) -> Result<Self, String> {
        if text.len() > 128 {
            return Err("Inovance address is too long".into());
        }
        let (unit, text) = split_unit(text)?;
        let text = text.to_ascii_uppercase();
        let text = text.strip_prefix('%').unwrap_or(&text);
        let mut address = match series {
            InovanceType::AM | InovanceType::AC | InovanceType::AP => Self::iec(text, bit, false)?,
            InovanceType::EVO => Self::iec(text, bit, true)?,
            _ => Self::soft_device(text, series, bit)?,
        };
        address.unit = unit;
        Ok(address)
    }

    fn range(
        kind: Kind,
        number: usize,
        first: usize,
        last: usize,
        base: usize,
    ) -> Result<Self, String> {
        if !(first..=last).contains(&number) {
            return Err(format!("Inovance device index must be in {first}..={last}"));
        }
        let offset = base + number - first;
        Ok(Self {
            kind,
            offset: u16::try_from(offset).map_err(|_| "Inovance address exceeds 65535")?,
            remaining: last - number + 1,
            unit: None,
            extended: false,
            read_only: false,
            counter32: false,
        })
    }

    fn iec(text: &str, bit: bool, evo: bool) -> Result<Self, String> {
        // AM400/AM600 用户手册“内置 Modbus 协议说明”：MD 按双字、MX 按字节寻址。
        // https://www.sbclinear.co.jp/databox/controller/am400/AM400_600_Manual.pdf
        if let Some(number) = text.strip_prefix("IX").or_else(|| text.strip_prefix('I')) {
            if !bit {
                return Err("Inovance I/IX requires bool".into());
            }
            let mut address = Self::range(Kind::DiscreteInput, byte_bit(number, 8)?, 0, 65535, 0)?;
            address.read_only = true;
            return Ok(address);
        }
        if evo && (text.starts_with("SM") || text.starts_with("SD")) {
            return Err("EVO does not use the AM600 SM/SD extended Modbus mapping".into());
        }
        if let Some(number) = text.strip_prefix("QX").or_else(|| text.strip_prefix('Q')) {
            if !bit {
                return Err("Inovance Q/QX requires bool".into());
            }
            return Self::range(Kind::Coil, byte_bit(number, 8)?, 0, 65535, 0);
        }
        if let Some(number) = text.strip_prefix("MX") {
            if !bit || !number.contains('.') {
                return Err("Inovance MX requires bool and MX<byte>.0..7".into());
            }
            let index = byte_bit(number, 8)?;
            return Self::range(Kind::WordBit((index % 16) as u8), index / 16, 0, 65535, 0);
        }
        if let Some(number) = text.strip_prefix("MB") {
            if bit {
                return Err("Inovance MB requires byte or string values; use MX for bool".into());
            }
            let index = decimal(number)?;
            return Self::range(Kind::Byte((index % 2) as u8), index / 2, 0, 65535, 0);
        }
        if let Some(number) = text.strip_prefix("SM") {
            if !bit {
                return Err("AM SM requires bool; SD is the system word area".into());
            }
            let mut address = Self::range(Kind::Coil, decimal(number)?, 0, 65535, 0)?;
            address.extended = true;
            return Ok(address);
        }
        let (number, scale, extended) = if let Some(number) =
            text.strip_prefix("SDW").or_else(|| text.strip_prefix("SD"))
        {
            (number, 1, true)
        } else if let Some(number) = text.strip_prefix("MW") {
            (number, 1, false)
        } else if let Some(number) = text.strip_prefix("MD") {
            (number, 2, false)
        } else {
            return Err("Inovance IEC addresses support I/IX, Q/QX, MX, MB, MW and MD; AM/AC/AP additionally support SM and SD/SDW".into());
        };
        let (number, kind) = word_address(number, bit)?;
        let index = number
            .checked_mul(scale)
            .ok_or("Inovance address overflow")?;
        let mut address = Self::range(kind, index, 0, 65535, 0)?;
        address.extended = extended;
        Ok(address)
    }

    fn soft_device(text: &str, series: InovanceType, bit: bool) -> Result<Self, String> {
        // 按 H3U、H5U/Easy 编程手册的 Modbus 软元件地址表；X/Y 均为八进制。
        // H3U C200 开始是双字计数器，物理寄存器起点 F700H，不与 C0..199 的 F400H 连续。
        let prefix_length = if text.starts_with("SM") || text.starts_with("SD") {
            2
        } else {
            1
        };
        if text.len() <= prefix_length || !text.is_ascii() {
            return Err("Invalid Inovance soft-device address".into());
        }
        let (prefix, number) = text.split_at(prefix_length);
        let h3u = series == InovanceType::H3U;
        if matches!(prefix, "X" | "Y") {
            if !bit {
                return Err("Inovance X/Y require bool".into());
            }
            let index = octal(number)?;
            let mut address = Self::range(
                Kind::Coil,
                index,
                0,
                if h3u { 255 } else { 1023 },
                if prefix == "X" { 0xf800 } else { 0xfc00 },
            )?;
            address.read_only = prefix == "X";
            return Ok(address);
        }
        if matches!(prefix, "M" | "SM" | "S" | "B") || (bit && matches!(prefix, "T" | "C")) {
            if !bit {
                return Err("Inovance relay areas require bool".into());
            }
            let index = decimal(number)?;
            let (first, last, base) = match (prefix, h3u) {
                ("M", true) if index >= 8000 => (8000, 8511, 0x1f40),
                ("M", true) => (0, 7679, 0),
                ("M", false) => (0, 7999, 0),
                ("SM", true) => (0, 1023, 0x2400),
                ("S", _) => (0, 4095, 0xe000),
                ("B", false) => (0, 32767, 0x3000),
                ("T", true) => (0, 511, 0xf000),
                ("C", true) => (0, 255, 0xf400),
                _ => return Err("Inovance relay is not supported by the selected series".into()),
            };
            return Self::range(Kind::Coil, index, first, last, base);
        }
        let (index, kind) = word_address(number, bit)?;
        let (last, base) = match (prefix, h3u) {
            ("D", true) => (8511, 0),
            ("D", false) => (7999, 0),
            ("R", _) => (32767, 0x3000),
            ("SD", true) => (1023, 0x2400),
            ("T", true) => (511, 0xf000),
            ("C", true) if index < 200 => (199, 0xf400),
            ("C", true) if index <= 255 => {
                let mut address = Self::range(kind, (index - 200) * 2, 0, 111, 0xf700)?;
                address.counter32 = true;
                return Ok(address);
            }
            _ => return Err("Inovance register is not supported by the selected series".into()),
        };
        Self::range(kind, index, 0, last, base)
    }

    pub fn validate(
        &self,
        width: usize,
        bit: bool,
        string: bool,
        count: usize,
        write: bool,
    ) -> Result<usize, String> {
        validate_shape(width, bit, string)?;
        if count == 0 {
            return Err("Inovance length must be greater than zero".into());
        }
        if write && self.read_only {
            return Err("Inovance X/I/IX input areas are read-only".into());
        }
        if self.counter32 && (bit || string || width != 4) {
            return Err("H3U C200..255 current values require 32-bit values (u32/i32/f32)".into());
        }
        let units = match self.kind {
            Kind::Coil | Kind::DiscreteInput if bit => count,
            Kind::WordBit(start) if bit => count
                .checked_add(usize::from(start))
                .ok_or("Inovance length overflow")?
                .div_ceil(16),
            Kind::Word if !bit => count
                .checked_mul(if string { 1 } else { width })
                .ok_or("Inovance length overflow")?
                .div_ceil(2),
            Kind::Byte(start) if !bit && (width == 1 || string) => count
                .checked_add(usize::from(start))
                .ok_or("Inovance length overflow")?
                .div_ceil(2),
            Kind::Byte(0) if !bit => {
                count.checked_mul(width).ok_or("Inovance length overflow")? / 2
            }
            _ => {
                return Err(
                    "Inovance address does not match value type; multi-byte MB values require an even byte address"
                        .into(),
                );
            }
        };
        if units > self.remaining {
            return Err("Inovance request crosses the selected device area boundary".into());
        }
        Ok(units)
    }

    pub fn modbus_address(&self, offset: usize, default_unit: u8) -> String {
        let prefix = match self.kind {
            Kind::Coil => "C",
            Kind::DiscreteInput => "DI",
            _ => "HR",
        };
        format!(
            "x={};{prefix}{}",
            self.unit.unwrap_or(default_unit),
            usize::from(self.offset) + offset
        )
    }
}

pub(super) fn validate_shape(width: usize, bit: bool, string: bool) -> Result<(), String> {
    if (string && !bit && width == 0)
        || (!string && ((bit && width == 1) || (!bit && matches!(width, 1 | 2 | 4 | 8))))
    {
        Ok(())
    } else {
        Err(
            "Inovance value must be bool, 1/2/4/8-byte scalar, or variable-length UTF-8 string"
                .into(),
        )
    }
}

fn decimal(text: &str) -> Result<usize, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Inovance index must be an unsigned decimal number".into());
    }
    text.parse()
        .map_err(|_| "Inovance address number overflow".into())
}

fn word_address(text: &str, bit: bool) -> Result<(usize, Kind), String> {
    if bit {
        let (word, position) = text
            .split_once('.')
            .ok_or("Register bool address requires .0..15")?;
        let position = decimal(position)?;
        if position > 15 {
            return Err("Inovance register bit must be in 0..=15".into());
        }
        Ok((decimal(word)?, Kind::WordBit(position as u8)))
    } else {
        Ok((decimal(text)?, Kind::Word))
    }
}

fn byte_bit(text: &str, radix: usize) -> Result<usize, String> {
    if let Some((byte, bit)) = text.split_once('.') {
        let bit = decimal(bit)?;
        if bit >= radix {
            return Err("Inovance byte bit must be in 0..=7".into());
        }
        decimal(byte)?
            .checked_mul(radix)
            .and_then(|base| base.checked_add(bit))
            .ok_or("Inovance address overflow".into())
    } else {
        decimal(text)
    }
}

fn octal(text: &str) -> Result<usize, String> {
    let digits = if let Some((group, bit)) = text.split_once('.') {
        if group.is_empty() || bit.len() != 1 {
            return Err("Inovance X/Y notation must be X10 or X1.0".into());
        }
        format!("{group}{bit}")
    } else {
        text.to_owned()
    };
    if digits.is_empty() || !digits.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
        return Err("Inovance X/Y addresses use octal digits 0..7".into());
    }
    usize::from_str_radix(&digits, 8).map_err(|_| "Inovance X/Y address overflow".into())
}

fn split_unit(text: &str) -> Result<(Option<u8>, &str), String> {
    let text = text.trim();
    let Some((prefix, address)) = text.split_once(';') else {
        return Ok((None, text));
    };
    let (name, value) = prefix
        .split_once('=')
        .ok_or("Inovance station prefix must be x=<unit>;address")?;
    if !(name.trim().eq_ignore_ascii_case("x") || name.trim().eq_ignore_ascii_case("s"))
        || address.contains(';')
        || address.trim().is_empty()
    {
        return Err(
            "Inovance station prefix must appear once as x=<unit>;address or s=<unit>;address"
                .into(),
        );
    }
    let unit = u8::try_from(decimal(value.trim())?).map_err(|_| "Inovance station exceeds 255")?;
    if unit == 0 {
        return Err("Inovance broadcast station zero is not supported".into());
    }
    Ok((Some(unit), address.trim()))
}
