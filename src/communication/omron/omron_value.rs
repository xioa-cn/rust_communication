#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ByteOrder {
    ABCD,
    BADC,
    #[default]
    CDAB,
    DCBA,
}

impl ByteOrder {
    pub(super) fn apply(self, bytes: &mut [u8]) {
        match self {
            Self::ABCD => {}
            Self::BADC => Self::swap_bytes(bytes),
            Self::CDAB => {
                bytes.reverse();
                Self::swap_bytes(bytes);
            }
            Self::DCBA => bytes.reverse(),
        }
    }

    pub(super) fn apply_byte_stream(self, bytes: &mut [u8]) {
        if matches!(self, Self::BADC | Self::DCBA) {
            Self::swap_bytes(bytes);
        }
    }

    fn swap_bytes(bytes: &mut [u8]) {
        for word in bytes.as_chunks_mut::<2>().0 {
            word.swap(0, 1);
        }
    }
}

pub trait OmronValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
    fn to_be_bytes(&self) -> Vec<u8>;
    fn append_be_bytes(&self, destination: &mut Vec<u8>) {
        destination.extend_from_slice(&self.to_be_bytes());
    }
}

pub trait OmronReadValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
}

impl<Value: OmronValue> OmronReadValue for Value {
    const BYTE_LEN: usize = Value::BYTE_LEN;
    const IS_BIT: bool = Value::IS_BIT;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Value::from_be_bytes(bytes)
    }
}

pub trait OmronWriteValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;
    fn encode(&self) -> Vec<u8>;
}

impl<Value: OmronValue> OmronWriteValue for Value {
    const BYTE_LEN: usize = Value::BYTE_LEN;
    const IS_BIT: bool = Value::IS_BIT;
    fn encode(&self) -> Vec<u8> {
        self.to_be_bytes()
    }
}

pub trait OmronWriteInput<Value> {
    fn into_value(self) -> Value;
}

impl<Value> OmronWriteInput<Value> for Value {
    fn into_value(self) -> Value {
        self
    }
}

impl OmronWriteInput<String> for &str {
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl OmronWriteInput<String> for &String {
    fn into_value(self) -> String {
        self.clone()
    }
}

macro_rules! numeric_value {
    ($($value_type:ty),+ $(,)?) => {
        $(impl OmronValue for $value_type {
            const BYTE_LEN: usize = std::mem::size_of::<Self>();
            fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
                let bytes = bytes.try_into().map_err(|_| "Omron.cs scalar byte length mismatch")?;
                Ok(<$value_type>::from_be_bytes(bytes))
            }
            fn to_be_bytes(&self) -> Vec<u8> { <$value_type>::to_be_bytes(*self).to_vec() }
            fn append_be_bytes(&self, destination: &mut Vec<u8>) {
                destination.extend_from_slice(&<$value_type>::to_be_bytes(*self));
            }
        })+
    };
}

numeric_value!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl OmronValue for bool {
    const BYTE_LEN: usize = 1;
    const IS_BIT: bool = true;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err("Omron.cs bit payload must be zero or one".into()),
        }
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
    fn append_be_bytes(&self, destination: &mut Vec<u8>) {
        destination.push(u8::from(*self));
    }
}

impl OmronReadValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("Omron.cs string is not valid UTF-8: {error}"))
    }
}

impl OmronWriteValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}
