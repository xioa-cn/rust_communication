pub use std::net::{IpAddr, Ipv4Addr, SocketAddr};
pub struct Config {
    ip_address: IpAddr,
    port: u16,
}

impl Config {
    pub fn new(ip_address: IpAddr, port: u16) -> Config {
        Config { ip_address, port }
    }

    pub fn get_ip_address(&self) -> &IpAddr {
        &self.ip_address
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn set_ip_address(&mut self, ip_address: IpAddr) {
        self.ip_address = ip_address;
    }

    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }
}
