pub(super) const MAX_PDU: usize = 253;

pub(super) fn range_request(function: u8, address: u16, count: u16) -> Vec<u8> {
    let mut request = Vec::with_capacity(5);
    request.push(function);
    request.extend_from_slice(&address.to_be_bytes());
    request.extend_from_slice(&count.to_be_bytes());
    request
}

pub(super) fn mbap_request(transaction: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(7 + pdu.len());
    frame.extend_from_slice(&transaction.to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&((pdu.len() + 1) as u16).to_be_bytes());
    frame.push(unit);
    frame.extend_from_slice(pdu);
    frame
}

pub(super) fn mbap_length(header: &[u8; 7], transaction: u16, unit: u8) -> Result<usize, String> {
    if u16::from_be_bytes([header[0], header[1]]) != transaction {
        return Err("Modbus transaction ID mismatch".into());
    }
    if header[2..4] != [0, 0] {
        return Err("Modbus protocol ID must be zero".into());
    }
    if header[6] != unit {
        return Err("Modbus unit ID mismatch".into());
    }
    let length = usize::from(u16::from_be_bytes([header[4], header[5]]));
    if !(2..=MAX_PDU + 1).contains(&length) {
        return Err("Modbus MBAP length is invalid".into());
    }
    Ok(length - 1)
}

pub(super) fn exception_message(code: u8) -> String {
    let description = match code {
        1 => "Illegal function",
        2 => "Illegal data address",
        3 => "Illegal data value",
        4 => "Server device failure",
        5 => "Acknowledge",
        6 => "Server device busy",
        8 => "Memory parity error",
        10 => "Gateway path unavailable",
        11 => "Gateway target device failed to respond",
        _ => "Unknown exception",
    };
    format!("Modbus exception 0x{code:02X}: {description}")
}

pub(super) fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

pub(super) fn lrc(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
        .wrapping_neg()
}
