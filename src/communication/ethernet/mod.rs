mod cip_address;
mod cip_client;
mod cip_transport;
pub mod cip_value;
pub mod inovance_cip;
pub mod melsec_cip;
pub mod omron_cip;

pub use cip_client::CipClient;
pub use cip_value::CipValue;
pub use inovance_cip::InovanceCip;
pub use melsec_cip::MelsecCip;
pub use omron_cip::OmronCip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipVendor {
    Omron,
    Melsec,
    Inovance,
}

#[derive(Debug, Clone)]
pub struct CipOptions {
    pub connected: bool,
    pub route: Vec<u8>,
    pub connection_size: u16,
    pub packet_interval_us: u32,
    pub timeout_multiplier: u8,
    pub originator_vendor_id: u16,
    pub originator_serial: u32,
}

impl Default for CipOptions {
    fn default() -> Self {
        Self {
            connected: false,
            route: Vec::new(),
            connection_size: 500,
            packet_interval_us: 2_000_000,
            timeout_multiplier: 2,
            originator_vendor_id: 0,
            originator_serial: std::process::id(),
        }
    }
}

impl CipOptions {
    pub fn connected() -> Self {
        Self {
            connected: true,
            connection_size: 1996,
            ..Self::default()
        }
    }
    pub fn with_slot(mut self, slot: u8) -> Self {
        self.route = vec![1, slot];
        self
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.route.len() > 500 || self.route.len() % 2 != 0 {
            return Err("CIP route must be an even EPATH of at most 500 bytes".into());
        }
        if !(128..=4000).contains(&self.connection_size)
            || (!self.connected && self.connection_size > 504)
        {
            return Err(
                "CIP message size must be 128..=504 for UCMM or 128..=4000 for Class 3".into(),
            );
        }
        if self.packet_interval_us == 0 || self.timeout_multiplier > 7 {
            return Err("CIP RPI must be positive and timeout multiplier must be 0..=7".into());
        }
        Ok(())
    }
}
