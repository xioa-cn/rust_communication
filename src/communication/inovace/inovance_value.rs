use super::ByteOrder;

/// 固定宽度值；支持 bool、8/16/32/64 位整数及 f32/f64。自定义实现须提供固定长度的大端编码。
pub trait InovanceValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
    fn to_be_bytes(&self) -> Vec<u8>;
}

/// String 读取的 length 表示 UTF-8 字节数，返回一个字符串元素。
pub trait InovanceReadValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
}

impl<Value: InovanceValue> InovanceReadValue for Value {
    const BYTE_LEN: usize = Value::BYTE_LEN;
    const IS_BIT: bool = Value::IS_BIT;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        Value::from_be_bytes(bytes)
    }
}

pub trait InovanceWriteValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;
    fn encode(&self) -> Vec<u8>;
}

impl<Value: InovanceValue> InovanceWriteValue for Value {
    const BYTE_LEN: usize = Value::BYTE_LEN;
    const IS_BIT: bool = Value::IS_BIT;
    fn encode(&self) -> Vec<u8> {
        self.to_be_bytes()
    }
}

/// 保持 `write::<String>(address, "text")` 与 S7/欧姆龙调用方式一致。
pub trait InovanceWriteInput<Value> {
    fn into_value(self) -> Value;
}

impl<Value> InovanceWriteInput<Value> for Value {
    fn into_value(self) -> Value {
        self
    }
}

impl InovanceWriteInput<String> for &str {
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl InovanceWriteInput<String> for &String {
    fn into_value(self) -> String {
        self.clone()
    }
}

macro_rules! numeric_value {
    ($($value_type:ty),+ $(,)?) => {
        $(impl InovanceValue for $value_type {
            const BYTE_LEN: usize = std::mem::size_of::<Self>();
            fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
                let bytes = bytes.try_into().map_err(|_| "Inovance scalar byte length mismatch")?;
                Ok(<$value_type>::from_be_bytes(bytes))
            }
            fn to_be_bytes(&self) -> Vec<u8> {
                <$value_type>::to_be_bytes(*self).to_vec()
            }
        })+
    };
}

numeric_value!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl InovanceValue for bool {
    const BYTE_LEN: usize = 1;
    const IS_BIT: bool = true;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err("Inovance bit payload must be zero or one".into()),
        }
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
}

impl InovanceReadValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("Inovance string is not valid UTF-8: {error}"))
    }
}

impl InovanceWriteValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

pub(super) fn transform(order: ByteOrder, bytes: &mut [u8], stream: bool) {
    if !stream && matches!(order, ByteOrder::CDAB | ByteOrder::DCBA) {
        bytes.reverse();
    }
    let swap = if stream {
        matches!(order, ByteOrder::BADC | ByteOrder::DCBA)
    } else {
        matches!(order, ByteOrder::BADC | ByteOrder::CDAB)
    };
    if swap {
        for word in bytes.as_chunks_mut::<2>().0 {
            word.swap(0, 1);
        }
    }
}
