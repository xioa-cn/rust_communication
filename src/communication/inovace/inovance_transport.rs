use crate::communication::modbus::ModbusTransport;

/// 仅翻译 AM 系统区功能码；MBAP、超时、响应长度及写回显仍由现有 Modbus 层校验。
pub(super) struct InovanceTransport<Transport> {
    pub inner: Transport,
    pub extended: bool,
}

impl<Transport: ModbusTransport> ModbusTransport for InovanceTransport<Transport> {
    fn connect(&mut self) -> Result<(), String> {
        self.inner.connect()
    }
    fn disconnect(&mut self) {
        self.inner.disconnect();
    }
    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }
    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        self.inner.validate_unit(unit)
    }

    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        if !self.extended {
            return self.inner.exchange(unit, request);
        }
        let Some(&function) = request.first() else {
            return Err("Inovance request cannot be empty".into());
        };
        if !matches!(function, 1 | 3 | 5 | 6 | 15 | 16) {
            return Err("Unsupported Inovance extended function".into());
        }
        let mut wire = request.to_vec();
        wire[0] = function + 0x30;
        let mut reply = self.inner.exchange(unit, &wire)?;
        match reply.first().copied() {
            Some(actual) if actual == wire[0] => reply[0] = function,
            Some(actual) if actual == (wire[0] | 0x80) => reply[0] = function | 0x80,
            _ => return Err("Inovance extended response function mismatch".into()),
        }
        Ok(reply)
    }
}
