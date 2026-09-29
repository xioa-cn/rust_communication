import { onMounted, ref } from 'vue'
import { isDesktop, s7 } from '../services/s7'
import { isSerialProtocol, protocolLabels } from '../types'
import type { ConnectRequest, ConnectionStatus, LogEntry, ReadRequest, ReadResult, WriteRequest } from '../types'
import type { Sample } from '../workbench'

export function useS7() {
  const connection = ref<ConnectionStatus>({ connected: false, protocol: null, cpu: null, pduLength: null })
  const busy = ref(false)
  const pending = ref('')
  const error = ref('')
  const result = ref<ReadResult | null>(null)
  const logs = ref<LogEntry[]>([])
  const samples = ref<Sample[]>([])
  let nextId = 0
  let sampleId = 0
  let endpoint = '未知（恢复的会话）'

  function log(level: LogEntry['level'], message: string) {
    logs.value.unshift({ id: ++nextId, time: new Date().toLocaleTimeString('zh-CN', { hour12: false }), level, message })
    logs.value = logs.value.slice(0, 100)
  }

  // 界面禁止并发提交；Rust 端仍以 Mutex 保证同一会话的完整操作串行执行。
  async function run<Value>(label: string, task: () => Promise<Value>): Promise<Value | null> {
    if (!isDesktop || busy.value) return null
    busy.value = true
    pending.value = label
    error.value = ''
    try {
      return await task()
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause)
      log('error', `${label}失败：${error.value}`)
      return null
    } finally {
      try { connection.value = await s7.status() }
      catch (cause) { log('error', `无法刷新本地连接状态：${String(cause)}`) }
      busy.value = false
      pending.value = ''
    }
  }

  async function connect(request: ConnectRequest) {
    await run('连接', async () => {
      connection.value = await s7.connect(request)
      result.value = null
      endpoint = isSerialProtocol(request.protocol) ? `${request.serial.path} / ${request.serial.baudRate} baud` : `${request.host}:${request.port}`
      log('ok', `已连接 ${endpoint} / ${protocolLabels[connection.value.protocol ?? request.protocol]}，${connection.value.pduLength ? `协商 PDU ${connection.value.pduLength} 字节` : '本地通讯资源已就绪'}`)
    })
  }
  async function disconnect() {
    await run('断开', async () => {
      connection.value = await s7.disconnect()
      result.value = null
      log('info', '连接已断开')
    })
  }
  async function read(request: ReadRequest) {
    return await run('读取', async () => {
      result.value = null
      const response = await s7.read(request)
      result.value = { ...response, address: request.address, dataType: request.dataType, time: new Date().toLocaleTimeString('zh-CN', { hour12: false }) }
      samples.value.push({ ...result.value, id: ++sampleId, time: new Date().toISOString(), protocol: connection.value.protocol, device: connection.value.cpu ?? connection.value.protocol ?? '未知', endpoint })
      if (samples.value.length > 1000) samples.value.splice(0, samples.value.length - 1000)
      log('ok', `读取 ${request.address} / ${request.dataType}：${response.values.length} 个结果，${response.elapsedMs} ms`)
      return response
    })
  }
  async function write(request: WriteRequest) {
    return await run('写入', async () => {
      const response = await s7.write(request)
      result.value = null
      log('ok', `写入 ${request.address}：${response.count} ${response.unit}，${response.elapsedMs} ms；请按需读取核对`)
      return response
    })
  }

  async function benchmark(request: ReadRequest, workers: number, count: number, stopped: () => boolean, progress: (elapsed: number) => void) {
    return await run('并发读取测试', async () => {
      if (!Number.isInteger(workers) || workers < 1 || workers > 8 || !Number.isInteger(count) || count < 1 || workers * count > 500) throw new Error('并发测试参数超出安全限制。')
      const started = performance.now()
      let completed = 0, failed = 0, aborted = false
      await Promise.all(Array.from({ length: workers }, async () => {
        for (let index = 0; index < count && !stopped() && !aborted; index++) {
          try {
            const response = await s7.read(request)
            completed++
            progress(response.elapsedMs)
            samples.value.push({ ...response, id: ++sampleId, address: request.address, dataType: request.dataType, time: new Date().toISOString(), protocol: connection.value.protocol, device: connection.value.cpu ?? connection.value.protocol ?? '未知', endpoint })
            if (samples.value.length > 1000) samples.value.splice(0, samples.value.length - 1000)
          } catch (cause) {
            failed++
            aborted = true
            log('error', `并发读取失败：${String(cause)}`)
          }
        }
      }))
      const report = { completed, failed, elapsedMs: performance.now() - started }
      log(failed ? 'error' : 'ok', `并发读取测试：完成 ${completed}，失败 ${failed}，${report.elapsedMs.toFixed(1)} ms；同一会话由后端锁串行执行`)
      return report
    })
  }

  onMounted(() => {
    log('info', isDesktop ? '工作台就绪，尚未主动连接或写入 PLC。' : '浏览器预览模式；请用 npm run tauri dev 启动桌面通讯。')
    if (isDesktop) void run('状态查询', async () => { connection.value = await s7.status() })
  })

  return { isDesktop, connection, busy, pending, error, result, logs, samples, connect, disconnect, read, write, benchmark,
    clearSamples: () => { samples.value = [] },
    clearLogs: () => { logs.value = [] } }
}
