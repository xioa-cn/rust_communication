use rs_appliaction::communication::melsec::MelsecNet;
use std::net::{IpAddr, Ipv4Addr};

fn get_melsec_local_test() -> MelsecNet {
    MelsecNet::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6000, Default::default())
}

#[test]
fn local_test() {
    let mut melsec_local_test = get_melsec_local_test();
    let connect_result = melsec_local_test.connect();
    if (connect_result.is_success) {
        println!("Connected");
    }

    let db = melsec_local_test.read::<i32>("D100", 1);

    if (db.is_success) {
        let v = db.content.unwrap()[0];
        assert_eq!(v, 12);
        println!("read D100 i32 {}", v);
    }

    let db = melsec_local_test.read::<i16>("D100", 1);

    if (db.is_success) {
        let v = db.content.unwrap()[0];
        assert_eq!(v, 12);
        println!("read D100 i16 {}", v);
    }
}
