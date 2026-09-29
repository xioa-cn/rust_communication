#[derive(Clone, Copy, Debug)]
pub(super) struct Address {
    pub code: u8,
    pub word: u16,
    pub bit: u8,
    pub is_bit: bool,
}

impl Address {
    pub fn parse(input: &str, is_bit: bool) -> Result<Self, String> {
        let text = input.trim().to_ascii_uppercase();
        let (word_code, bit_code, offset) = if let Some(rest) = text.strip_prefix('E') {
            let rest = rest.strip_prefix('M').unwrap_or(rest);
            let (bank, offset) = rest
                .split_once('.')
                .ok_or("Omron.cs EM address requires E<hex bank>.<word>[.<bit>]")?;
            if bank.is_empty()
                || bank.len() > 2
                || !bank.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("Invalid Omron.cs EM bank".into());
            }
            let bank = u8::from_str_radix(bank, 16).map_err(|_| "Invalid Omron.cs EM bank")?;
            match bank {
                0..=15 => (0xa0 + bank, 0x20 + bank, offset),
                16..=24 => (0x60 + bank - 16, 0xe0 + bank - 16, offset),
                _ => return Err("Omron.cs EM bank must be hexadecimal 0..18".into()),
            }
        } else {
            let areas = [
                ("CIO", 0xb0, 0x30),
                ("DM", 0x82, 0x02),
                ("WR", 0xb1, 0x31),
                ("HR", 0xb2, 0x32),
                ("AR", 0xb3, 0x33),
                ("D", 0x82, 0x02),
                ("C", 0xb0, 0x30),
                ("W", 0xb1, 0x31),
                ("H", 0xb2, 0x32),
                ("A", 0xb3, 0x33),
            ];
            areas
                .into_iter()
                .find_map(|(prefix, word, bit)| {
                    text.strip_prefix(prefix).map(|offset| (word, bit, offset))
                })
                .ok_or("Unsupported Omron.cs area; use D, CIO/C, W, H, A or E<bank>.<word>")?
        };
        let (word, bit) = match offset.split_once('.') {
            Some((word, bit)) if is_bit => (word, decimal(bit)?),
            Some(_) => return Err("Omron.cs word/byte/string access cannot use a bit suffix".into()),
            None => (offset, 0),
        };
        if bit > 15 {
            return Err("Omron.cs bit index must be in 0..=15".into());
        }
        Ok(Self {
            code: if is_bit { bit_code } else { word_code },
            word: decimal(word)?,
            bit: bit as u8,
            is_bit,
        })
    }

    pub fn validate_length(self, count: usize) -> Result<(), String> {
        let capacity =
            (usize::from(u16::MAX) + 1 - usize::from(self.word)) * if self.is_bit { 16 } else { 1 };
        let available = capacity - usize::from(self.bit);
        if count == 0 || count > available {
            return Err("Omron.cs count is zero or crosses the 16-bit memory address range".into());
        }
        Ok(())
    }

    pub fn advance(self, offset: usize) -> Self {
        let (words, bit) = if self.is_bit {
            (
                (usize::from(self.bit) + offset) / 16,
                (usize::from(self.bit) + offset) % 16,
            )
        } else {
            (offset, 0)
        };
        Self {
            word: self.word + words as u16,
            bit: bit as u8,
            ..self
        }
    }
}

fn decimal(text: &str) -> Result<u16, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Omron.cs word and bit offsets must be unsigned decimal numbers".into());
    }
    text.parse()
        .map_err(|_| "Omron.cs address offset exceeds 65535".into())
}
