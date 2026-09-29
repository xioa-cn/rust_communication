use rs_appliaction::common::config;
use std::net::{IpAddr, Ipv4Addr};
#[test]
fn it_works() {
    let v4_ip = Ipv4Addr::new(127, 0, 0, 1);
    let config = config::Config::new(IpAddr::V4(v4_ip), 502);
    let config_v4_ip = config.get_ip_address();

    match config_v4_ip {
        IpAddr::V4(v4_ip) => {
            assert_eq!(config_v4_ip, v4_ip);
        }
        _ => panic!("IPV4 设置错误"),
    }

    assert_eq!(config.get_port(), 502);
}
