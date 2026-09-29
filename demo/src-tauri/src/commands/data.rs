use crate::{
    models::{ReadRequest, ReadResponse, WriteRequest, WriteResponse},
    plc,
    state::{self, PlcState},
};
use std::time::Instant;
use tauri::State;

#[tauri::command]
pub async fn read(
    state: State<'_, PlcState>,
    request: ReadRequest,
) -> Result<ReadResponse, String> {
    state::execute(state, move |current| {
        let started = Instant::now();
        let values = plc::read(plc::connected(current)?, &request)?;
        Ok(ReadResponse {
            values,
            elapsed_ms: started.elapsed().as_millis(),
        })
    })
    .await
}

#[tauri::command]
pub async fn write(
    state: State<'_, PlcState>,
    request: WriteRequest,
) -> Result<WriteResponse, String> {
    state::execute(state, move |current| {
        let started = Instant::now();
        let (count, unit) = plc::write(plc::connected(current)?, &request)?;
        Ok(WriteResponse {
            count,
            unit,
            elapsed_ms: started.elapsed().as_millis(),
        })
    })
    .await
}
