use super::PlcClient;
use rs_appliaction::entity::operate::Operator;

/// 转成可跨线程、可序列化的错误；避免 Operator::to_result 中的缺值 panic。
pub fn result<T>(operator: Operator<T>) -> Result<T, String> {
    if operator.is_success {
        operator
            .content
            .ok_or_else(|| "库返回成功，但缺少结果内容".into())
    } else {
        Err(operator.msg)
    }
}

pub fn connected(client: &mut Option<PlcClient>) -> Result<&mut PlcClient, String> {
    client
        .as_mut()
        .filter(|client| client.is_connected())
        .ok_or_else(|| "请先连接 PLC".into())
}
