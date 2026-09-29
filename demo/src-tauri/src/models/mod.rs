mod connection;
mod data;

pub use connection::{
    ConnectRequest, ConnectionStatus, CpuModel, ModbusByteOrder, ModbusOptions, OmronOptions,
    PlcProtocol, SerialOptions, SerialParity,
};
pub use data::{DataType, ReadRequest, ReadResponse, WriteMode, WriteRequest, WriteResponse};

#[cfg(test)]
pub use connection::InovanceOptions;
