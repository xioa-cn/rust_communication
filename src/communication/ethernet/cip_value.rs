mod sealed {
    pub trait Sealed {}
}

pub(crate) fn type_name(type_code: u16) -> &'static str {
    match type_code {
        0xc1 => "BOOL/bool",
        0xc2 => "SINT/i8",
        0xc3 => "INT/i16",
        0xc4 => "DINT/i32",
        0xc5 => "LINT/i64",
        0xc6 => "USINT/u8",
        0xc7 => "UINT/u16",
        0xc8 => "UDINT/u32",
        0xc9 => "ULINT/u64",
        0xca => "REAL/f32 (Float)",
        0xcb => "LREAL/f64 (Double)",
        0xd0 => "STRING/UTF-8",
        0xd1 => "BYTE/u8",
        0xd2 => "WORD/u16",
        0xd3 => "DWORD/u32",
        0xd4 => "LWORD/u64",
        _ => "unsupported tag type",
    }
}

pub trait CipValue: sealed::Sealed + Copy {
    const TYPE_CODE: u16;
    const WIDTH: usize;
    fn accepts(type_code: u16) -> bool;
    fn append(self, destination: &mut Vec<u8>);
    fn decode(bytes: &[u8]) -> Result<Self, String>;
}

macro_rules! number {
    ($kind:ty, $code:expr $(, $alias:expr)?) => {
        impl sealed::Sealed for $kind {}
        impl CipValue for $kind {
            const TYPE_CODE: u16 = $code;
            const WIDTH: usize = size_of::<Self>();
            fn accepts(type_code: u16) -> bool { type_code == $code $(|| type_code == $alias)? }
            fn append(self, destination: &mut Vec<u8>) { destination.extend_from_slice(&self.to_le_bytes()); }
            fn decode(bytes: &[u8]) -> Result<Self, String> {
                Ok(Self::from_le_bytes(bytes.try_into().map_err(|_| "Invalid CIP element length")?))
            }
        }
    };
}

number!(i8, 0xc2);
number!(i16, 0xc3);
number!(i32, 0xc4);
number!(i64, 0xc5);
number!(u8, 0xc6, 0xd1);
number!(u16, 0xc7, 0xd2);
number!(u32, 0xc8, 0xd3);
number!(u64, 0xc9, 0xd4);
number!(f32, 0xca);
number!(f64, 0xcb);

impl sealed::Sealed for bool {}
impl CipValue for bool {
    const TYPE_CODE: u16 = 0xc1;
    const WIDTH: usize = 1;
    fn accepts(type_code: u16) -> bool {
        type_code == Self::TYPE_CODE
    }
    fn append(self, destination: &mut Vec<u8>) {
        destination.push(u8::from(self));
    }
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] | [0xff] => Ok(true),
            _ => Err("Invalid CIP BOOL representation".into()),
        }
    }
}
