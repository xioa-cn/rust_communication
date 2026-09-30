use rs_appliaction::communication::melsec::MelsecValue;
use rs_appliaction::communication::modbus::ModbusValue;
use rs_appliaction::communication::omron::OmronValue;
use rs_appliaction::communication::s7::s7_value::S7Value;

#[test]
fn direct_append_preserves_numeric_bytes_and_existing_prefix() {
    macro_rules! check {
        ($value:expr) => {{
            let value = $value;
            let mut little = vec![0xa5];
            MelsecValue::append_le_bytes(&value, &mut little);
            assert_eq!(little[0], 0xa5);
            assert_eq!(&little[1..], &MelsecValue::to_le_bytes(&value));
            let mut big = vec![0xa5];
            S7Value::append_be_bytes(&value, &mut big);
            OmronValue::append_be_bytes(&value, &mut big);
            assert_eq!(big[0], 0xa5);
            assert_eq!(
                &big[1..],
                &[
                    S7Value::to_be_bytes(&value),
                    OmronValue::to_be_bytes(&value)
                ]
                .concat()
            );
        }};
    }
    macro_rules! check_all {
        ($value:expr) => {{
            check!($value);
            let value = $value;
            let mut bytes = vec![0xa5];
            ModbusValue::append_be_bytes(&value, &mut bytes);
            assert_eq!(bytes[0], 0xa5);
            assert_eq!(&bytes[1..], &ModbusValue::to_be_bytes(&value));
        }};
    }
    check!(u8::MAX);
    check!(i8::MIN);
    check_all!(u16::MAX);
    check_all!(i16::MIN);
    check_all!(u32::MAX);
    check_all!(i32::MIN);
    check_all!(u64::MAX);
    check_all!(i64::MIN);
    check_all!(-0.0f32);
    check_all!(f32::from_bits(0x7fc12345));
    check_all!(f64::NEG_INFINITY);
    check_all!(f64::from_bits(0x7ff8123456789abc));
    check_all!(true);
    check_all!(false);
}
