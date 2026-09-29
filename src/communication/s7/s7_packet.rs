//! COTP 握手及 S7 报文构造、响应头校验；所有方法均不执行网络操作。

use super::s7_address::S7Address;

pub(super) const REQUESTED_PDU: usize = 960;
/// 读响应：12 字节响应头 + 2 字节参数 + 4 字节数据项头。
pub(super) const READ_OVERHEAD: usize = 18;
/// 写请求：10 字节请求头 + 14 字节参数 + 4 字节数据项头。
pub(super) const WRITE_OVERHEAD: usize = 28;

/// 构造 COTP 连接请求，TSAP 由 CPU 型号和组态决定，不在报文层硬编码。
pub(super) fn connection_request(local_tsap: u16, remote_tsap: u16) -> Vec<u8> {
    let mut request = vec![17, 0xe0, 0, 0, 1, 0, 0, 0xc1, 2];
    request.extend_from_slice(&local_tsap.to_be_bytes());
    request.extend_from_slice(&[0xc2, 2]);
    request.extend_from_slice(&remote_tsap.to_be_bytes());
    request.extend_from_slice(&[0xc0, 1, 10]);
    request
}

/// 验证 COTP 连接确认的长度、类型、目标引用及参数边界。
pub(super) fn validate_connection_confirm(packet: &[u8]) -> Result<(), String> {
    if packet.len() < 7
        || usize::from(packet[0]) + 1 != packet.len()
        || packet[1] != 0xd0
        || packet[2..4] != [1, 0]
        || packet[6] != 0
    {
        return Err("Invalid COTP Connection Confirm".into());
    }
    let mut offset = 7;
    while offset < packet.len() {
        if packet.len() - offset < 2 {
            return Err("Truncated COTP connection parameter".into());
        }
        let code = packet[offset];
        let length = usize::from(packet[offset + 1]);
        let end = offset + 2 + length;
        if end > packet.len() {
            return Err("Truncated COTP connection parameter value".into());
        }
        // 请求使用 1024 字节 TPDU，足以容纳最大 960 字节 S7 PDU。
        if code == 0xc0 && (length != 1 || packet[offset + 2] != 10) {
            return Err("Expected a negotiated COTP TPDU size of 1024 bytes".into());
        }
        offset = end;
    }
    Ok(())
}

/// Setup Communication：单个未完成请求，并申请最大 960 字节 PDU。
pub(super) fn setup_parameters() -> Vec<u8> {
    let mut parameters = vec![0xf0, 0, 0, 1, 0, 1];
    parameters.extend_from_slice(&(REQUESTED_PDU as u16).to_be_bytes());
    parameters
}

/// 校验协商结果；后续分块必须使用 PLC 实际返回的 PDU 大小。
pub(super) fn negotiated_pdu(response: &Response) -> Result<usize, String> {
    response.check_error()?;
    let parameters = &response.parameters;
    // 第二个参数字节是保留字段，部分服务返回 0x01，不能据此判定协商失败。
    // 仍严格验证功能码、参数长度、并发数及数据区，PDU 长度另行检查。
    if parameters.len() != 8
        || parameters[0] != 0xf0
        || read_u16(&parameters[2..4]) == 0
        || read_u16(&parameters[4..6]) == 0
        || !response.data.is_empty()
    {
        return Err(format!(
            "Invalid S7 Setup Communication response: expected function F0, \
             8 parameter bytes, nonzero AMQ limits and empty data; \
             parameters={:02X?}, data={:02X?}",
            parameters, response.data
        ));
    }
    let length = usize::from(read_u16(&parameters[6..8]));
    if !(240..=REQUESTED_PDU).contains(&length) {
        return Err(format!("Unsupported negotiated PDU length: {length}"));
    }
    Ok(length)
}

/// 封装单个 S7 Job，返回带 COTP Data 头但不带 TPKT 头的数据。
pub(super) fn job(sequence: u16, parameters: &[u8], data: &[u8]) -> Vec<u8> {
    let mut request = vec![2, 0xf0, 0x80, 0x32, 1, 0, 0];
    request.extend_from_slice(&sequence.to_be_bytes());
    request.extend_from_slice(&(parameters.len() as u16).to_be_bytes());
    request.extend_from_slice(&(data.len() as u16).to_be_bytes());
    request.extend_from_slice(parameters);
    request.extend_from_slice(data);
    request
}

/// 构造 Read Var / Write Var 的单项 S7ANY 参数。
/// 位操作的 offset/count 按位计数，其他操作按字节计数。
pub(super) fn variable_parameters(
    address: &S7Address,
    function: u8,
    offset: usize,
    count: usize,
) -> Vec<u8> {
    let transport = if address.bit.is_some() { 1 } else { 2 };
    let mut parameters = vec![function, 1, 0x12, 0x0a, 0x10, transport];
    parameters.extend_from_slice(&(count as u16).to_be_bytes());
    parameters.extend_from_slice(&address.db.to_be_bytes());
    parameters.push(address.area);
    parameters.extend_from_slice(&address.bit_address(offset).to_be_bytes()[1..]);
    parameters
}

/// 写数据项：返回码占位、传输类型、以位为单位的长度及有效载荷。
pub(super) fn write_data(bytes: &[u8], is_bit: bool) -> Vec<u8> {
    let bit_length = if is_bit { 1 } else { bytes.len() * 8 };
    let mut data = vec![0, if is_bit { 3 } else { 4 }];
    data.extend_from_slice(&(bit_length as u16).to_be_bytes());
    data.extend_from_slice(bytes);
    data
}

/// 已通过结构和序列号校验的 S7 响应。
pub(super) struct Response {
    pub(super) parameters: Vec<u8>,
    pub(super) data: Vec<u8>,
    error: u16,
}

impl Response {
    /// 验证响应头、序列号及参数/数据总长度，避免消费错配或截断的响应。
    pub(super) fn parse(packet: &[u8], sequence: u16) -> Result<Self, String> {
        if packet.len() < 12
            || packet[0] != 0x32
            || !matches!(packet[1], 2 | 3)
            || packet[2..4] != [0, 0]
            || read_u16(&packet[4..6]) != sequence
        {
            return Err("Invalid S7 response header or sequence number".into());
        }
        let parameter_length = usize::from(read_u16(&packet[6..8]));
        let data_length = usize::from(read_u16(&packet[8..10]));
        if packet.len() != 12 + parameter_length + data_length {
            return Err("S7 response length mismatch".into());
        }
        Ok(Self {
            parameters: packet[12..12 + parameter_length].to_vec(),
            data: packet[12 + parameter_length..].to_vec(),
            error: read_u16(&packet[10..12]),
        })
    }

    /// PLC 业务错误不等于报文损坏，调用方可以保留连接继续使用。
    pub(super) fn check_error(&self) -> Result<(), String> {
        if self.error == 0 {
            Ok(())
        } else {
            Err(format!("PLC returned S7 error 0x{:04X}", self.error))
        }
    }
}

/// 将数据项返回码转换为可诊断的错误信息，并保留原始错误码。
pub(super) fn item_error(code: u8) -> String {
    let message = match code {
        0x01 => "hardware fault",
        0x03 => "access denied",
        0x05 => "address out of range",
        0x06 => "unsupported data type",
        0x07 => "data type mismatch",
        0x0a => "object does not exist",
        _ => "unknown item error",
    };
    format!("PLC returned item error 0x{code:02X}: {message}")
}

/// 读取报文中的大端 u16；调用方必须先检查切片长度。
pub(super) fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

#[cfg(test)]
mod tests {
    use super::{Response, negotiated_pdu};

    #[test]
    fn accepts_captured_setup_response_with_nonzero_reserved_byte() {
        // 本地 1020 服务的实际 S7 响应，已去除 TPKT/COTP 头。
        let packet = [
            0x32, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x01,
            0x00, 0x01, 0x00, 0xf0, 0x00, 0xf0,
        ];
        let response = Response::parse(&packet, 1).unwrap();
        assert_eq!(negotiated_pdu(&response).unwrap(), 240);
    }

    #[test]
    fn ignores_only_the_reserved_setup_parameter() {
        for reserved in [0x00, 0x01, 0xff] {
            let response = Response {
                parameters: vec![0xf0, reserved, 0, 1, 0, 1, 0, 240],
                data: vec![],
                error: 0,
            };
            assert_eq!(negotiated_pdu(&response).unwrap(), 240);
        }
    }

    #[test]
    fn rejects_malformed_setup_responses_with_diagnostic_bytes() {
        for parameters in [
            vec![],
            vec![0xf0, 1, 0, 1, 0, 1, 0],
            vec![0xf0, 1, 0, 1, 0, 1, 0, 240, 0],
            vec![0x04, 1, 0, 1, 0, 1, 0, 240],
            vec![0xf0, 1, 0, 0, 0, 1, 0, 240],
            vec![0xf0, 1, 0, 1, 0, 0, 0, 240],
        ] {
            let response = Response {
                parameters,
                data: vec![],
                error: 0,
            };
            let error = negotiated_pdu(&response).unwrap_err();
            assert!(error.contains("Invalid S7 Setup Communication response"));
            assert!(error.contains("parameters="));
        }
        let response = Response {
            parameters: vec![0xf0, 1, 0, 1, 0, 1, 0, 240],
            data: vec![0x12],
            error: 0,
        };
        assert!(negotiated_pdu(&response).unwrap_err().contains("data=[12]"));
    }

    #[test]
    fn still_rejects_plc_errors_and_invalid_pdu_sizes() {
        let mut response = Response {
            parameters: vec![0xf0, 1, 0, 1, 0, 1, 0, 240],
            data: vec![],
            error: 0x8104,
        };
        assert!(negotiated_pdu(&response).unwrap_err().contains("0x8104"));
        response.error = 0;
        for length in [0_u16, 239, 961] {
            response.parameters[6..8].copy_from_slice(&length.to_be_bytes());
            assert!(
                negotiated_pdu(&response)
                    .unwrap_err()
                    .contains("PDU length")
            );
        }
    }
}
