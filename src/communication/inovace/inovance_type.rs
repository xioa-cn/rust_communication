/// 汇川内置 Modbus 地址映射；不同系列不能混用软元件地址。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InovanceType {
    /// AM 系列，与 AC/AP 共用内置 Modbus 地址映射。
    #[default]
    AM,
    /// AC 系列，与 AM/AP 使用同一套通讯与地址解析逻辑。
    AC,
    /// AP 系列，与 AM/AC 使用同一套通讯与地址解析逻辑。
    AP,
    /// EVO 系列：IEC 地址及独立输入区，不复用 AM600 的 SM/SD 扩展。
    EVO,
    /// H5U：M/B/S/X/Y 位区，D/R 字区。
    H5U,
    /// H3U：含 SM/SD、定时器及 16/32 位计数器。
    H3U,
    /// Easy 系列沿用 H5U 的内置 Modbus 软元件映射，并非 EasyNet 协议。
    Easy,
}
