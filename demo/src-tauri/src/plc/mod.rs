mod client;
mod cip;
mod connection;
mod read;
mod response;
mod serial;
mod validation;
mod write;

pub use client::PlcClient;
pub use connection::create_client;
pub use read::read;
pub use response::{connected, result};
pub use write::write;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod modbus_tests;

#[cfg(test)]
mod inovance_tests;
#[cfg(test)]
mod omron_tests;
