#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ByteOrder {
    #[default]
    ABCD,
    BADC,
    CDAB,
    DCBA,
}

impl ByteOrder {
    pub(super) fn apply(self, bytes: &mut [u8]) {
        match self {
            Self::ABCD => {}
            Self::BADC => {
                for word in bytes.chunks_exact_mut(2) {
                    word.swap(0, 1);
                }
            }
            Self::CDAB => {
                bytes.reverse();
                for word in bytes.chunks_exact_mut(2) {
                    word.swap(0, 1);
                }
            }
            Self::DCBA => bytes.reverse(),
        }
    }
}

pub trait ModbusValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
    fn to_be_bytes(&self) -> Vec<u8>;
}

fn fixed_bytes<const SIZE: usize>(bytes: &[u8]) -> Result<[u8; SIZE], String> {
    bytes
        .try_into()
        .map_err(|_| "Modbus value byte length mismatch".into())
}

impl ModbusValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;

    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("Modbus string is not valid UTF-8: {error}"))
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl ModbusValue for bool {
    const BYTE_LEN: usize = 1;
    const IS_BIT: bool = true;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err("Modbus bit value must be zero or one".into()),
        }
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Vec::from([u8::from(*self)])
    }
}

impl ModbusValue for u16 {
    const BYTE_LEN: usize = 2;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for i16 {
    const BYTE_LEN: usize = 2;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for u32 {
    const BYTE_LEN: usize = 4;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for i32 {
    const BYTE_LEN: usize = 4;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for u64 {
    const BYTE_LEN: usize = 8;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for i64 {
    const BYTE_LEN: usize = 8;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for f32 {
    const BYTE_LEN: usize = 4;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}

impl ModbusValue for f64 {
    const BYTE_LEN: usize = 8;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self::from_be_bytes(fixed_bytes(bytes)?))
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Self::to_be_bytes(*self).to_vec()
    }
}
