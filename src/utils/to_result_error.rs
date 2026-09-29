
use std::error::Error;
use std::fmt;

/// 通用业务字符串错误，用于Operator转换
#[derive(Debug, Clone)]
pub struct BizMsgError(pub String);

impl fmt::Display for BizMsgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for BizMsgError {}