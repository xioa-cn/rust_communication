//! S7 绝对地址解析与边界检查，不负责网络收发或报文封装。

/// S7 使用 24 位的位地址，因此最大字节偏移为 0x1FFFFF。
const MAX_BYTE_ADDRESS: usize = 0x1f_ffff;

/// 解析后的地址，仅供 S7 模块内部使用。
pub(super) struct S7Address {
    pub(super) area: u8,
    pub(super) db: u16,
    pub(super) byte_offset: usize,
    pub(super) bit: Option<u8>,
}

impl S7Address {
    /// 支持 DB1.0、DB1.DBW0、DB1.DBX0.1、M0、I0、Q0 等格式。
    /// V/VB/VW/VD/VX 为 DB1 的别名，例如 VW100 等同于 DB1.DBW100。
    /// E/A 分别作为 I/Q 的别名；地址中的 B/W/D 只描述语法，元素类型由 T 决定。
    /// bool 简写省略位号时默认第 0 位，例如 DB1.100 等同于 DB1.100.0。
    /// 数值简写可带末尾 .0，例如 DB1.100.0；非零位偏移不允许用于数值读写。
    pub(super) fn parse(text: &str, is_bit: bool) -> Result<Self, String> {
        let text = text.trim().to_ascii_uppercase();
        let (area, db, rest) = if let Some(rest) = text.strip_prefix("DB") {
            let (number, offset) = rest.split_once('.').ok_or("Expected DB<number>.<offset>")?;
            let db = u16::try_from(decimal(number)?).map_err(|_| "DB number exceeds 65535")?;
            (0x84, db, offset.strip_prefix("DB").unwrap_or(offset))
        } else if let Some(rest) = text.strip_prefix('V') {
            (0x84, 1, rest)
        } else {
            let area = match text.as_bytes().first() {
                Some(b'I' | b'E') => 0x81,
                Some(b'Q' | b'A') => 0x82,
                Some(b'M') => 0x83,
                _ => return Err("Supported areas are DB, V, M, I/E and Q/A".into()),
            };
            (area, 0, &text[1..])
        };
        let (prefix, rest) = match rest.as_bytes().first() {
            Some(b'B' | b'W' | b'D' | b'X') => (rest.as_bytes().first().copied(), &rest[1..]),
            _ => (None, rest),
        };
        let (byte_offset, bit) = match rest.split_once('.') {
            Some((offset, bit)) => {
                let bit = decimal(bit)?;
                if bit > 7 {
                    return Err("Bit index must be 0..7".into());
                }
                (decimal(offset)?, Some(bit as u8))
            }
            None => (decimal(rest)?, None),
        };
        if prefix == Some(b'X') && bit.is_none() {
            return Err("An X address requires an explicit bit index".into());
        }
        if matches!(prefix, Some(b'B' | b'W' | b'D')) && bit.is_some() {
            return Err("Byte/word/dword addresses cannot have a bit index".into());
        }
        if byte_offset > MAX_BYTE_ADDRESS {
            return Err("Address exceeds the 24-bit S7 bit-address range".into());
        }
        // 地址语义取决于本次访问类型，不能把 DB1.100 中的 100 当成位号。
        let bit = if is_bit {
            Some(bit.unwrap_or(0))
        } else if bit == Some(0) && prefix != Some(b'X') {
            None
        } else {
            bit
        };
        Ok(Self {
            area,
            db,
            byte_offset,
            bit,
        })
    }

    /// 验证完整操作范围，避免分块发送后才发现后续地址溢出。
    /// length 对位操作表示位数，对其他操作表示字节数。
    pub(super) fn validate(&self, length: usize, is_bit: bool) -> Result<(), String> {
        if self.bit.is_some() != is_bit {
            return Err("Use bool for bit addresses and non-bool values for byte addresses".into());
        }
        let available = if is_bit {
            (MAX_BYTE_ADDRESS + 1 - self.byte_offset) * 8 - usize::from(self.bit.unwrap_or(0))
        } else {
            MAX_BYTE_ADDRESS + 1 - self.byte_offset
        };
        if length == 0 || length > available {
            return Err("Length is zero or exceeds the S7 address range".into());
        }
        Ok(())
    }

    /// 将分块偏移换算为线上使用的位地址；调用前必须验证整个操作范围。
    /// 位数组的 offset 按位递增，其余数据按字节递增。
    pub(super) fn bit_address(&self, offset: usize) -> u32 {
        let base = self.byte_offset * 8 + usize::from(self.bit.unwrap_or(0));
        (base
            + if self.bit.is_some() {
                offset
            } else {
                offset * 8
            }) as u32
    }
}

/// 仅接受十进制数字，拒绝负号、空串及溢出的地址分量。
fn decimal(text: &str) -> Result<usize, String> {
    if text.is_empty() || !text.bytes().all(|value| value.is_ascii_digit()) {
        return Err(format!("Invalid decimal address component: {text:?}"));
    }
    text.parse()
        .map_err(|_| "Address number is too large".into())
}

#[cfg(test)]
mod tests {
    use super::S7Address;

    #[test]
    fn boolean_shorthand_preserves_byte_offset_and_defaults_to_bit_zero() {
        for text in ["DB1.100", "DB1.100.0", "DB1.DBX100.0", " db1.100 "] {
            let address = S7Address::parse(text, true).unwrap();
            assert_eq!(address.area, 0x84);
            assert_eq!(address.db, 1);
            assert_eq!(address.byte_offset, 100);
            assert_eq!(address.bit, Some(0));
            assert_eq!(address.bit_address(0), 800);
            assert_eq!(address.bit_address(8), 808);
        }
    }

    #[test]
    fn numeric_shorthand_with_zero_bit_suffix_remains_byte_oriented() {
        for text in ["DB1.100", "DB1.100.0", "DB1.DBW100"] {
            let address = S7Address::parse(text, false).unwrap();
            assert_eq!(address.bit, None);
            assert_eq!(address.bit_address(0), 800);
            assert_eq!(address.bit_address(2), 816);
            assert!(address.validate(2, false).is_ok());
        }
        let address = S7Address::parse("DB1.100.1", false).unwrap();
        assert!(address.validate(2, false).is_err());
    }

    #[test]
    fn shorthand_keeps_bit_and_address_boundaries() {
        let address = S7Address::parse("M2097151", true).unwrap();
        assert!(address.validate(8, true).is_ok());
        assert!(address.validate(9, true).is_err());
        for text in ["DB1.100.8", "M100.8", "DB1.100.-1", "M2097152"] {
            assert!(S7Address::parse(text, true).is_err(), "{text}");
        }
    }
}
