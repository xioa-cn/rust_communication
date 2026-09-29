use super::omron_address::Address;

pub(super) const MAX_FRAME: usize = 2012;
pub(super) const MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinsRoute {
    pub destination_network: u8,
    pub destination_node: u8,
    pub destination_unit: u8,
    pub source_network: u8,
    pub source_node: u8,
    pub source_unit: u8,
    pub gateway_count: u8,
}

impl Default for FinsRoute {
    fn default() -> Self {
        Self {
            destination_network: 0,
            destination_node: 0,
            destination_unit: 0,
            source_network: 0,
            source_node: 0,
            source_unit: 0,
            gateway_count: 2,
        }
    }
}

impl FinsRoute {
    pub(super) fn validate(self, resolved: bool) -> Result<(), String> {
        if self.destination_network > 127 || self.source_network > 127 || self.gateway_count > 7 {
            return Err("FINS network numbers must be 0..=127 and gateway count 0..=7".into());
        }
        if self.destination_node == 255 || self.source_node == 255 {
            return Err("FINS node 255 is not supported; broadcasts are not acknowledged".into());
        }
        if resolved && (self.destination_node == 0 || self.source_node == 0) {
            return Err(
                "FINS node addresses must resolve to 1..=254; configure the route explicitly"
                    .into(),
            );
        }
        if self.destination_network != 0 && self.destination_node == 0 {
            return Err("FINS routed destinations require an explicit destination node".into());
        }
        Ok(())
    }
}

pub(super) fn memory_request(
    route: FinsRoute,
    sid: u8,
    address: Address,
    count: usize,
    data: Option<&[u8]>,
) -> Vec<u8> {
    let mut frame = vec![
        0x80,
        0,
        route.gateway_count,
        route.destination_network,
        route.destination_node,
        route.destination_unit,
        route.source_network,
        route.source_node,
        route.source_unit,
        sid,
        1,
        if data.is_some() { 2 } else { 1 },
        address.code,
    ];
    frame.extend_from_slice(&address.word.to_be_bytes());
    frame.push(address.bit);
    frame.extend_from_slice(&(count as u16).to_be_bytes());
    if let Some(data) = data {
        frame.extend_from_slice(data);
    }
    frame
}

pub(super) fn validate_response(request: &[u8], reply: &[u8]) -> Result<u16, String> {
    if !(14..=MAX_FRAME).contains(&reply.len()) {
        return Err("FINS response length is invalid".into());
    }
    if reply[0] & 0xc0 != 0xc0 {
        return Err("FINS response header is invalid".into());
    }
    if reply[3..6] != request[6..9] || reply[6..9] != request[3..6] {
        return Err("FINS response route does not match the request".into());
    }
    if reply[9] != request[9] {
        return Err("FINS response SID mismatch".into());
    }
    if reply[10..12] != request[10..12] {
        return Err("FINS response command mismatch".into());
    }
    Ok(u16::from_be_bytes([reply[12], reply[13]]))
}

pub(super) fn end_code_message(code: u16) -> String {
    let reason = match code {
        0x0101 => "local node not in network",
        0x0102 => "token timeout",
        0x0201 => "destination node not in network",
        0x0205 => "response timeout",
        0x0304 => "CPU error",
        0x0401 => "undefined command",
        0x0402 => "unsupported command",
        0x1001 => "command too long",
        0x1002 => "command too short",
        0x1003 => "element count mismatch",
        0x1101 => "invalid memory area",
        0x1102 => "invalid access size",
        0x1103 => "address out of range",
        0x1104 => "address range exceeded",
        0x2101 => "write protected",
        _ => "controller rejected the command or reported a status error",
    };
    format!("Omron.cs FINS end code 0x{code:04X}: {reason}")
}

pub(super) fn tcp_frame(command: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = b"FINS".to_vec();
    frame.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&[0; 4]);
    frame.extend_from_slice(payload);
    frame
}
