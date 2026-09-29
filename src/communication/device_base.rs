use crate::entity::operate::Operator;

/// 设备连接的生命周期。实现方在实例中持有连接状态。
pub trait DeviceBase {
    /// 建立连接；失败原因通过 `Operator.msg` 返回。
    fn connect(&mut self) -> Operator<bool>;
    /// 释放连接；允许重复调用。
    fn disconnect(&mut self) -> Operator<bool>;
}

/// 通用读取接口；具体协议通过实现此 trait 限定支持的数据类型。
pub trait ReadBase<T> {
    /// 读取数据，返回拥有所有权、长度固定的堆数组。
    /// 地址语法及 length 的单位由具体协议和目标类型约定。
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[T]>>;
}

/// 通用写入接口，不限定协议采用的字节序或数据表示。
pub trait WriteBase<T> {
    /// 写入单个值，成功后返回原值。
    fn write(&mut self, address: &str, value: T) -> Operator<T>;
    /// 连续写入数组或切片，成功后返回写入的元素个数；不转移数组所有权。
    fn write_all(&mut self, address: &str, array: &[T]) -> Operator<usize>;
}
