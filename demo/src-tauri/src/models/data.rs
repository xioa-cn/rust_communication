use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    RawString,
    S7String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadRequest {
    pub address: String,
    pub data_type: DataType,
    pub length: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    Single,
    Array,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequest {
    pub address: String,
    pub data_type: DataType,
    pub mode: WriteMode,
    /// 使用文本传递整数，避免 JavaScript 丢失 i64/u64 的精度。
    pub values: Vec<String>,
    pub confirmed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResponse {
    pub values: Box<[String]>,
    pub elapsed_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResponse {
    pub count: usize,
    pub unit: &'static str,
    pub elapsed_ms: u128,
}
