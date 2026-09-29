use crate::entity::operate::Operator;

/// Socket 超时配置，单位为毫秒；连接前由设备实现校验其有效性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeout {
    connect_time_out: i32,
    receive_time_out: i32,
}

impl Timeout {
    /// 创建超时配置；两个参数都必须大于零。
    pub fn new(connect_time_out: i32, receive_time_out: i32) -> Self {
        Self {
            connect_time_out,
            receive_time_out,
        }
    }

    /// TCP 建立连接的超时时间，单位为毫秒。
    pub fn connect_time_out(&self) -> i32 {
        self.connect_time_out
    }

    /// Socket 接收超时时间，单位为毫秒；实现方也可将其用于发送超时。
    pub fn receive_time_out(&self) -> i32 {
        self.receive_time_out
    }
}

impl Default for Timeout {
    /// 默认连接、接收超时均为 5 秒。
    fn default() -> Self {
        Self::new(5_000, 5_000)
    }
}

pub fn as_operator<T>(result: Result<T, String>) -> Operator<T> {
    match result {
        Ok(value) => Operator::ok(value),
        Err(error) => Operator::err(&error),
    }
}

