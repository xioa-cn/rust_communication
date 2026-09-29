/// PLC 系列标识；使用经典 S7 协议进行绝对地址读写。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S7Type {
    S1200,
    S1500,
    /// 使用实际 CPU 的 rack/slot；常见 S7-300 为 rack=0、slot=2。
    S300,
    /// slot 由硬件组态决定，不覆盖调用方提供的机架和槽位。
    S400,
    /// 经典 S7-200 需配置支持 S7 over TCP 的以太网模块/网关；不支持串口 PPI。
    S200,
    /// S7-200 SMART 通过以太网使用 S7；V 区对应 DB1。
    S200Smart,
}

impl S7Type {
    /// 默认 TSAP 参照 S7NetPlus 的 TsapPair.GetDefaultTsapPair。
    /// S200 的 TSAP 取决于以太网模块项目组态，可通过 with_tsap 显式覆盖。
    pub(super) fn default_tsap(self, rack: usize, slot: usize) -> Result<(u16, u16), String> {
        if rack > 7 || slot > 31 {
            return Err("Rack must be 0..7 and slot must be 0..31".into());
        }
        match self {
            Self::S200 => Ok((0x1000, 0x1001)),
            Self::S1200 | Self::S1500 | Self::S300 | Self::S400 | Self::S200Smart => {
                Ok((0x0100, 0x0300 | ((rack as u16) << 5) | slot as u16))
            }
        }
    }

    /// S200/SMART STRING 只有实际长度；其他系列为最大容量+实际长度。
    pub(super) fn string_header_length(self) -> usize {
        match self {
            Self::S200 | Self::S200Smart => 1,
            _ => 2,
        }
    }
}
