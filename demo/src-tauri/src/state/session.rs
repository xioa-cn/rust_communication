//! Mutex 覆盖整次操作，避免多个线程交叉消费同一 Socket 的响应。
use crate::models::ConnectionStatus;
use crate::plc::PlcClient;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Default)]
pub struct PlcState(pub Arc<Mutex<Option<PlcClient>>>);

pub fn snapshot(client: &Option<PlcClient>) -> ConnectionStatus {
    match client {
        Some(client) => ConnectionStatus {
            connected: client.is_connected(),
            protocol: Some(client.protocol()),
            cpu: client.cpu(),
            pdu_length: client.negotiated_pdu_length(),
            inovance_series: client.inovance_series(),
        },
        None => ConnectionStatus::default(),
    }
}

/// 锁等待和同步网络 I/O 一起放进阻塞线程池，避免阻塞 WebView/异步线程。
/// 断开连接同样会等待当前完整操作结束，不会打断已经发出的写入。
pub async fn execute<T, Work>(state: State<'_, PlcState>, work: Work) -> Result<T, String>
where
    T: Send + 'static,
    Work: FnOnce(&mut Option<PlcClient>) -> Result<T, String> + Send + 'static,
{
    let shared = Arc::clone(&state.inner().0);
    tauri::async_runtime::spawn_blocking(move || {
        let mut client = shared.lock().map_err(|_| "连接状态锁已损坏，请重启应用")?;
        work(&mut client)
    })
    .await
    .map_err(|error| format!("后台通讯任务失败：{error}"))?
}
