//! Rust 类型与 S7 大端有效载荷的转换，不涉及 Socket、地址或报文头。

/// S7 读写的定长元素；read::<T>(address, length) 返回 length 个 T。
pub trait S7Value: Sized {
    /// 一个元素的字节数。bool 解码时占一个字节，但在 PLC 地址中只占一位。
    const BYTE_LEN: usize;
    /// 是否采用位寻址及位传输。
    const IS_BIT: bool = false;

    /// 从一个元素的大端有效载荷解码；字节数不匹配时返回错误。
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
    /// 将一个元素编码为大端有效载荷。
    fn to_be_bytes(&self) -> Vec<u8>;
    fn append_be_bytes(&self, destination: &mut Vec<u8>) {
        destination.extend_from_slice(&self.to_be_bytes());
    }
}

/// 可读取的 S7 元素。String 将指定长度的原始字节解码为一个字符串。
/// String 的写入使用独立的容量校验流程，不按定长数值处理。
pub trait S7ReadValue: Sized {
    /// 定长元素的字节宽度；String 的宽度由调用者指定，设置为 0。
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    /// 为 true 时，read 的 length 为字节数，返回数组中仅包含一个字符串。
    const IS_STRING: bool = false;

    /// 解码一个元素的内容；String 不自动识别或跳过任何长度头。
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String>;
}

/// 现有数值和 bool 自动实现读取接口，无需重复维护它们的转换规则。
impl<T: S7Value> S7ReadValue for T {
    const BYTE_LEN: usize = T::BYTE_LEN;
    const IS_BIT: bool = T::IS_BIT;

    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        T::from_be_bytes(bytes)
    }
}

/// 单值写入的编码约定；字符串由会话层先验证 PLC 中的 STRING 容量。
pub trait S7WriteValue: Sized {
    const BYTE_LEN: usize;
    const IS_BIT: bool = false;
    const IS_STRING: bool = false;

    /// 数值返回大端字节；String 返回不含长度头的 UTF-8 正文。
    fn encode(&self) -> Vec<u8>;
}

impl<T: S7Value> S7WriteValue for T {
    const BYTE_LEN: usize = T::BYTE_LEN;
    const IS_BIT: bool = T::IS_BIT;

    fn encode(&self) -> Vec<u8> {
        self.to_be_bytes()
    }
}

/// 明确限定输入到返回类型的转换，支持 write::<String>(address, &str)。
/// 不使用宽泛的 Into<T>，避免数值写入在多个可转换目标之间产生类型推断歧义。
pub trait S7WriteInput<T> {
    fn into_value(self) -> T;
}

impl<T> S7WriteInput<T> for T {
    fn into_value(self) -> T {
        self
    }
}

impl S7WriteInput<String> for &str {
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl S7WriteInput<String> for &String {
    fn into_value(self) -> String {
        self.clone()
    }
}

// 使用 Rust 标准库大端转换，避免结果受本机字节序影响。
macro_rules! numeric_value {
    ($($value_type:ty),+ $(,)?) => {
        $(impl S7Value for $value_type {
            const BYTE_LEN: usize = std::mem::size_of::<Self>();

            fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
                let bytes = bytes.try_into().map_err(|_| "Invalid scalar byte length")?;
                Ok(<$value_type>::from_be_bytes(bytes))
            }

            fn to_be_bytes(&self) -> Vec<u8> {
                <$value_type>::to_be_bytes(*self).to_vec()
            }
            fn append_be_bytes(&self, destination: &mut Vec<u8>) {
                destination.extend_from_slice(&<$value_type>::to_be_bytes(*self));
            }
        })+
    };
}

numeric_value!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl S7Value for bool {
    const BYTE_LEN: usize = 1;
    const IS_BIT: bool = true;

    /// 位读取仅接受 0 或 1，避免将异常数据静默解释为 true。
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err("Invalid boolean payload".into()),
        }
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
    fn append_be_bytes(&self, destination: &mut Vec<u8>) {
        destination.push(u8::from(*self));
    }
}
