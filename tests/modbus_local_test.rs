use std::net::{IpAddr, Ipv4Addr};
use rs_appliaction::communication::modbus::{ByteOrder, ModbusTcp};

#[test]
fn test(){
    let mut modbus  = 
        ModbusTcp::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0, Default::default());
    modbus.set_byte_order(ByteOrder::CDAB);
}