use rs_appliaction::communication::s7::s7_net::S7Net;
use rs_appliaction::communication::s7::s7_type::S7Type;
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr};

fn get_s7net() -> S7Net {
    S7Net::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        1020,
        S7Type::S1200,
        Default::default(),
        0,
        0,
    )
}

#[test]
fn s7_test1() {
    let mut s7net = get_s7net();

    let connect_result = s7net.connect();

    if (connect_result.is_success) {
        println!("Connection successful");
    } else {
        println!("Connection failed{}", connect_result.msg);
    }

    let db1 = s7net.read::<bool>("DB1.1.0", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received bool: {}", t[0]);
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.read::<i16>("DB1.1", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received i16: {}", t[0]);
    }

    let db1 = s7net.read::<i32>("DB1.1", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received i32: {}", t[0]);
    }

    let db1 = s7net.read::<i64>("DB1.1", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received i64: {}", t[0]);
    }

    let db1 = s7net.read::<f32>("DB1.1", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received f32: {}", t[0]);
    }

    let db1 = s7net.read::<f64>("DB1.1", 1);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received f64: {}", t[0]);
    }

    // 当前测试数据的 STRING 头在 DB1.10，正文从 DB1.12 开始，共 5 字节。
    let db1 = s7net.read::<String>("DB1.12", 10);

    if (db1.is_success) {
        let t = db1.content.unwrap();
        let t1 = t[0].to_string();
        println!("DB1 received String: {}", t1.replace("\0", "-"));
    } else {
        println!("DB1 failed String {}", db1.msg);
    }

    let db1 = s7net.read_s7_strings("DB1.10");

    if (db1.is_success) {
        let t = db1.content.unwrap();
        println!("DB1 received read_s7_strings: {}", t[0]);
    } else {
        println!("DB1 failed read_s7_strings {}", db1.msg);
    }
}

#[test]
fn s7_test2() {
    let mut s7net = get_s7net();

    let connect_result = s7net.connect();

    if (connect_result.is_success) {
        println!("Connection successful");
    }

    let db1 = s7net.write::<bool>("DB1.1.0", true);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<i16>("DB1.1", 16800);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<i32>("DB1.1", 1101004800);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<i64>("DB1.1", 4728779608739020800);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<f32>("DB1.1", 20f32);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<f64>("DB1.1", 134217728f64);
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }

    let db1 = s7net.write::<String>("DB1.10","nihao");
    if (db1.is_success) {
        println!("DB1 successfull");
    } else {
        println!("DB1 failed bool {}", db1.msg);
    }
}


