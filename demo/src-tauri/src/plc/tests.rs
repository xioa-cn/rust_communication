use super::{
    connected, result,
    validation::{MAX_ITEMS, validate_address, validate_count},
    write::{parse_values, validate_write},
};
use crate::models::{DataType, WriteMode, WriteRequest};
use rs_appliaction::entity::operate::Operator;
fn request() -> WriteRequest {
    WriteRequest {
        address: "DB1.0".into(),
        data_type: DataType::U16,
        mode: WriteMode::Single,
        values: vec!["123".into()],
        confirmed: true,
    }
}
#[test]
fn integers_preserve_all_64_bits() {
    assert_eq!(
        parse_values::<u64>(&[u64::MAX.to_string()]).unwrap(),
        vec![u64::MAX]
    );
    assert_eq!(
        parse_values::<i64>(&[i64::MIN.to_string()]).unwrap(),
        vec![i64::MIN]
    );
    assert!(parse_values::<u8>(&["256".into()]).is_err());
}
#[test]
fn rejects_invalid_bool_and_nonfinite_float() {
    assert_eq!(
        parse_values::<bool>(&["true".into(), "false".into()]).unwrap(),
        vec![true, false]
    );
    assert!(parse_values::<bool>(&["1".into()]).is_err());
    assert!(parse_values::<f32>(&["NaN".into()]).is_err());
    assert!(parse_values::<f64>(&["inf".into()]).is_err());
}
#[test]
fn write_requires_confirmation_and_matching_shape() {
    let mut value = request();
    assert!(validate_write(&value).is_ok());
    value.confirmed = false;
    assert!(validate_write(&value).is_err());
    value.confirmed = true;
    value.values.push("456".into());
    assert!(validate_write(&value).is_err());
    value.mode = WriteMode::Array;
    assert!(validate_write(&value).is_ok());
    value.values.clear();
    assert!(validate_write(&value).is_err());
}
#[test]
fn empty_string_is_only_valid_with_s7_header() {
    let mut value = request();
    value.values = vec![String::new()];
    value.data_type = DataType::S7String;
    assert!(validate_write(&value).is_ok());
    value.data_type = DataType::RawString;
    assert!(validate_write(&value).is_err());
}
#[test]
fn errors_do_not_panic_or_require_network() {
    assert!(
        result::<u8>(Operator {
            content: None,
            msg: "success".into(),
            is_success: true
        })
        .is_err()
    );
    assert!(connected(&mut None).is_err());
    assert!(validate_count(0).is_err());
    assert!(validate_count(MAX_ITEMS + 1).is_err());
}

#[test]
fn address_validation_rejects_blank_and_oversized_inputs() {
    assert!(validate_address(" ").is_err());
    assert!(validate_address(&"A".repeat(129)).is_err());
    assert!(validate_address("DB1.0").is_ok());
}

#[test]
fn input_limits_are_preserved_after_module_split() {
    let mut value = request();
    value.values = vec!["1".repeat(MAX_ITEMS + 1)];
    assert!(validate_write(&value).is_err());
    value.mode = WriteMode::Array;
    value.values = vec!["1".into(); MAX_ITEMS + 1];
    assert!(validate_write(&value).is_err());
    value.values = vec!["1".into(); MAX_ITEMS];
    assert!(validate_write(&value).is_ok());
}
