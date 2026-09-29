//! 三菱小端值转换；与 S7 的泛型 API 一致，但字节序和字符串存储格式独立。

/// 固定宽度数值或 bool；按元素数读取，u8/i8 不浪费每个字的高字节。
pub trait MelsecValue: Sized {
    /// 定长值的字节宽度；可变长字符串设为零。
    const BYTE_LEN: usize;
    /// 是否按位设备访问；内部每位用一个 0/1 字节表示。
    const IS_BIT: bool = false;
    /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String>;
    /// 编码为低字节、低字在前的连续字节流。
    fn to_le_bytes(&self) -> Vec<u8>;
}

/// 读取能力；String 使用调用方指定的原始字节长度。
pub trait MelsecReadValue: Sized {
    /// 定长值的字节宽度；可变长字符串设为零。
    const BYTE_LEN: usize;
    /// 是否按位设备访问；内部每位用一个 0/1 字节表示。
    const IS_BIT: bool = false;
    /// 是否由调用方提供字节长度或由文本正文决定长度。
    const IS_STRING: bool = false;
    /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String>;
}

impl<T: MelsecValue> MelsecReadValue for T {
    const BYTE_LEN: usize = T::BYTE_LEN;
    const IS_BIT: bool = T::IS_BIT;
    /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String> {
        T::from_le_bytes(bytes)
    }
}

/// 单值写入编码约定；String 不生成 PLC 专用长度头。
pub trait MelsecWriteValue: Sized {
    /// 定长值的字节宽度；可变长字符串设为零。
    const BYTE_LEN: usize;
    /// 是否按位设备访问；内部每位用一个 0/1 字节表示。
    const IS_BIT: bool = false;
    /// 是否由调用方提供字节长度或由文本正文决定长度。
    const IS_STRING: bool = false;
    /// 编码正文，不包含协议帧头或字符串长度头。
    fn encode(&self) -> Vec<u8>;
}

impl<T: MelsecValue> MelsecWriteValue for T {
    const BYTE_LEN: usize = T::BYTE_LEN;
    const IS_BIT: bool = T::IS_BIT;
    /// 编码正文，不包含协议帧头或字符串长度头。
    fn encode(&self) -> Vec<u8> {
        self.to_le_bytes()
    }
}

/// 明确输入类型转换，支持字符串借用而不让数值推断产生歧义。
pub trait MelsecWriteInput<T> {
    /// 转换成写入后返回给调用方的拥有所有权的值。
    fn into_value(self) -> T;
}

impl<T> MelsecWriteInput<T> for T {
    /// 转换成写入后返回给调用方的拥有所有权的值。
    fn into_value(self) -> T {
        self
    }
}

impl MelsecWriteInput<String> for &str {
    /// 转换成写入后返回给调用方的拥有所有权的值。
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl MelsecWriteInput<String> for &String {
    /// 转换成写入后返回给调用方的拥有所有权的值。
    fn into_value(self) -> String {
        self.clone()
    }
}

/// 使用标准库转换，避免依赖主机字节序或手写符号扩展。
macro_rules! numeric_value {
    ($($value_type:ty),+ $(,)?) => {
        $(impl MelsecValue for $value_type {
            const BYTE_LEN: usize = std::mem::size_of::<Self>();
            /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String> {
                let bytes = bytes.try_into().map_err(|_| "Invalid MELSEC scalar byte length")?;
                Ok(<$value_type>::from_le_bytes(bytes))
            }
            /// 编码为低字节、低字在前的连续字节流。
    fn to_le_bytes(&self) -> Vec<u8> {
                <$value_type>::to_le_bytes(*self).to_vec()
            }
        })+
    };
}

numeric_value!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl MelsecValue for bool {
    const BYTE_LEN: usize = 1;
    const IS_BIT: bool = true;
    /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err("Invalid MELSEC boolean payload".into()),
        }
    }
    /// 编码为低字节、低字在前的连续字节流。
    fn to_le_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
}

impl MelsecReadValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    /// 严格按小端序解码，错误长度或非法内容返回错误。
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("Invalid MELSEC UTF-8 text: {error}"))
    }
}

impl MelsecWriteValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;
    /// 编码正文，不包含协议帧头或字符串长度头。
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}
