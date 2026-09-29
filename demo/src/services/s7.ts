import {invoke, isTauri} from '@tauri-apps/api/core'
import type {ConnectRequest, ConnectionStatus, ReadRequest, ReadResponse, WriteRequest, WriteResponse} from '../types'

export const isDesktop = isTauri()

// Vue 只调用自定义命令；S7、MC、A1E 和 MC-R 报文始终留在 Rust 端。
export const s7 = {
    connect: (request: ConnectRequest) => invoke<ConnectionStatus>('connect', {request}),
    disconnect: () => invoke<ConnectionStatus>('disconnect'),
    status: () => invoke<ConnectionStatus>('status'),
    read: (request: ReadRequest) => invoke<ReadResponse>('read', {request}),
    write: (request: WriteRequest) => invoke<WriteResponse>('write', {request}),
}
