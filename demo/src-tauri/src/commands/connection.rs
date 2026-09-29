use crate::{
    models::{ConnectRequest, ConnectionStatus},
    plc,
    state::{self, PlcState},
};
use tauri::State;

#[tauri::command]
pub async fn connect(
    state: State<'_, PlcState>,
    request: ConnectRequest,
) -> Result<ConnectionStatus, String> {
    state::execute(state, move |current| {
        if current.as_ref().is_some_and(|client| client.is_connected()) {
            return Err("已有连接，请先断开再更改连接参数".into());
        }
        let mut client = plc::create_client(request)?;
        plc::result(client.connect())?;
        *current = Some(client);
        Ok(state::snapshot(current))
    })
    .await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, PlcState>) -> Result<ConnectionStatus, String> {
    state::execute(state, |current| {
        if let Some(mut client) = current.take() {
            plc::result(client.disconnect())?;
        }
        Ok(ConnectionStatus::default())
    })
    .await
}

/// 只返回本地状态，不是主动心跳；远端断线可能到下次 I/O 才发现。
#[tauri::command]
pub async fn status(state: State<'_, PlcState>) -> Result<ConnectionStatus, String> {
    state::execute(state, |current| Ok(state::snapshot(current))).await
}
