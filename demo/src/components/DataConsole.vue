<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { ArrowDownOutline, ArrowUpOutline, PauseOutline, PulseOutline } from '@vicons/ionicons5'
import { availableDataTypes, defaultAddress, isModbusProtocol, isReadOnlyAddress } from '../types'
import type { DataType, InovanceSeries, PlcProtocol, ReadRequest, ReadResponse, WriteRequest, WriteResponse } from '../types'
import { createSerialRunner, decodeText, displayValue, encodeText, parseHex, parseValues, quickTypes, reversePairs } from '../workbench'
import type { NumberDisplay, TextEncoding } from '../workbench'
import type { RustOperation } from '../rustExamples'

const props = defineProps<{ protocol: PlcProtocol; inovanceSeries?: InovanceSeries; connected: boolean; desktop: boolean; busy: boolean; locked: boolean; read: (request: ReadRequest) => Promise<ReadResponse | null>; write: (request: WriteRequest) => Promise<WriteResponse | null> }>()
const emit = defineEmits<{ running: [value: boolean]; example: [operation: RustOperation, reveal: boolean] }>()
const readForm = reactive({ address: defaultAddress(props.protocol, 'u16', props.inovanceSeries), type: 'u16' as DataType, length: 1, interval: 1000, encoding: 'utf-8' as TextEncoding, reverse: false })
const writeForm = reactive({ address: defaultAddress(props.protocol, 'u16', props.inovanceSeries), type: 'u16' as DataType, text: '', mode: 'values', interval: 1000, count: 10, encoding: 'utf-8' as TextEncoding, reverse: false, increment: false, start: 1, end: 100 })
const display = ref<NumberDisplay>('dec'), filter = ref(''), suppressDuplicates = ref(false), curve = ref(false), confirmed = ref(false), message = ref(''), writeResult = ref('')
const values = ref<string[]>([]), trend = ref<number[]>([]), running = ref(false), taskName = ref('')
const lastReadContext = ref<{ address: string; type: DataType; time: string } | null>(null)
const timings = reactive({ read: [] as number[], write: [] as number[] })
const counts = reactive({ read: 0, write: 0, changes: 0 })
const runner = createSerialRunner(value => { running.value = value; emit('running', value); if (!value) { taskName.value = ''; confirmed.value = false } })
const modbus = computed(() => isModbusProtocol(props.protocol))
const available = computed(() => availableDataTypes(props.protocol).map(option => option.value))
const editable = computed(() => !props.busy && !props.locked)
const ready = computed(() => props.desktop && props.connected && editable.value)
const resultLines = computed(() => values.value.map((value, index) => ({ index, text: displayValue(value, display.value) })).filter(row => !filter.value || row.text.toLowerCase().includes(filter.value.toLowerCase())))
const encodings = computed(() => modbus.value ? ['utf-8', 'ascii'] : ['utf-8', 'ascii', 'utf-16le', 'utf-16be'])
const polyline = computed(() => {
  const points = trend.value, minimum = Math.min(...points), span = Math.max(...points) - minimum || 1
  return points.map((value, index) => `${index * 600 / Math.max(points.length - 1, 1)},${90 - (value - minimum) / span * 70}`).join(' ')
})
function statistics(kind: 'read' | 'write') {
  const entries = timings[kind]
  return entries.length ? { last: entries.at(-1)!, min: Math.min(...entries), max: Math.max(...entries), average: (entries.reduce((sum, value) => sum + value, 0) / entries.length).toFixed(1) } : { last: '—', min: '—', max: '—', average: '—' }
}
function record(kind: 'read' | 'write', elapsed: number) {
  counts[kind]++
  timings[kind].push(elapsed)
  if (timings[kind].length > 500) timings[kind].shift()
}
function selectType(kind: 'read' | 'write', type: DataType) {
  const form = kind === 'read' ? readForm : writeForm
  if (form.address === defaultAddress(props.protocol, form.type, props.inovanceSeries)) form.address = defaultAddress(props.protocol, type, props.inovanceSeries)
  form.type = type
}
function checkAddress(address: string) {
  if (!address.trim() || new TextEncoder().encode(address).length > 128) throw new Error('地址不能为空，且最多 128 字节。')
}
function readRequest(): ReadRequest {
  checkAddress(readForm.address)
  if (!Number.isInteger(readForm.length) || readForm.length < 1 || readForm.length > 1024) throw new Error('读取数量必须为 1–1024。')
  return { address: readForm.address.trim(), dataType: readForm.type === 'raw_string' && !modbus.value ? 'u8' : readForm.type, ...(readForm.type === 's7_string' ? {} : { length: readForm.length }) }
}
async function readOnce(request: ReadRequest, options = { ...readForm }): Promise<boolean> {
  emit('example', { kind: 'read', request, ...(options.type === 'raw_string' && !modbus.value ? { text: { encoding: options.encoding, reverse: options.reverse } } : {}) }, false)
  const response = await props.read(request)
  if (!response) return false
  let next = response.values
  if (options.type === 'raw_string') {
    if (!modbus.value) {
      const bytes = Uint8Array.from(response.values.map(Number))
      next = [decodeText(options.reverse ? reversePairs(bytes) : bytes, options.encoding)]
    } else if (options.encoding === 'ascii') encodeText(next.join(''), 'ascii')
  }
  record('read', response.elapsedMs)
  lastReadContext.value = { address: request.address, type: options.type, time: new Date().toLocaleTimeString('zh-CN', { hour12: false }) }
  if (!suppressDuplicates.value || JSON.stringify(next) !== JSON.stringify(values.value)) {
    values.value = next
    counts.changes++
    const number = Number(next[0])
    if (!['raw_string', 's7_string', 'bool'].includes(options.type) && Number.isFinite(number)) { trend.value.push(number); if (trend.value.length > 120) trend.value.shift() }
  }
  return true
}
function writeRequest(index = 0): WriteRequest {
  checkAddress(writeForm.address)
  if (isReadOnlyAddress(props.protocol, writeForm.address)) throw new Error('该输入地址为只读区域，不能写入。')
  let type = writeForm.type, text = writeForm.text
  if (writeForm.increment) {
    if (!Number.isSafeInteger(writeForm.start) || !Number.isSafeInteger(writeForm.end) || writeForm.end < writeForm.start || !/^[uif]\d+$/.test(type)) throw new Error('自增写入需要数值类型，起止值为安全整数且终值不小于起值。')
    text = String(writeForm.start + index % (writeForm.end - writeForm.start + 1))
  }
  let output: string[]
  if (writeForm.mode === 'hex') {
    if (modbus.value) throw new Error('Modbus 请按寄存器类型写入；原始字节写入尚未暴露。')
    type = 'u8'
    output = [...parseHex(text)].map(String)
  } else if (type === 'raw_string' && !modbus.value) {
    let bytes = encodeText(text, writeForm.encoding)
    if (writeForm.reverse) bytes = reversePairs(bytes)
    type = 'u8'
    output = [...bytes].map(String)
  } else {
    if (type === 'raw_string') encodeText(text, writeForm.encoding)
    output = parseValues(text, type)
  }
  if (!output.length || output.length > 1024 || (type === 'raw_string' && (!text || encodeText(text, 'utf-8').length > 1024))) throw new Error('写入数据不能为空，且最多 1024 项 / 字节。')
  return { address: writeForm.address.trim(), dataType: type, mode: ['raw_string', 's7_string'].includes(type) || output.length === 1 ? 'single' : 'array', values: output, confirmed: true }
}
async function perform(kind: 'read' | 'write', repeat = false) {
  if (!ready.value || (kind === 'write' && !confirmed.value)) return
  message.value = ''
  try {
    const request = kind === 'read' ? readRequest() : writeRequest()
    if (repeat && (!Number.isInteger(kind === 'read' ? readForm.interval : writeForm.interval) || (kind === 'read' ? readForm.interval : writeForm.interval) < 200)) throw new Error('定时间隔至少 200 ms。')
    if (repeat && kind === 'write' && (!Number.isInteger(writeForm.count) || writeForm.count < 1 || writeForm.count > 10000)) throw new Error('定时写入次数必须为 1–10000。')
    const options = { ...readForm }
    emit('example', kind === 'read' ? { kind: 'read', request: request as ReadRequest } : { kind: 'write', request: request as WriteRequest }, true)
    const job = async (index: number) => {
      if (!props.connected) return false
      if (kind === 'read') return readOnce(request as ReadRequest, options)
      const nextRequest = writeForm.increment ? writeRequest(index) : request as WriteRequest
      emit('example', { kind: 'write', request: nextRequest }, false)
      const response = await props.write(nextRequest)
      if (!response) return false
      record('write', response.elapsedMs)
      writeResult.value = `已写入 ${response.count} ${response.unit} · ${response.elapsedMs} ms`
      return true
    }
    if (repeat) {
      taskName.value = kind === 'read' ? '定时读取' : '定时写入'
      await runner.start(job, kind === 'read' ? readForm.interval : writeForm.interval, kind === 'read' ? 0 : writeForm.count)
    } else { await job(0); if (kind === 'write') confirmed.value = false }
  } catch (error) { message.value = String(error instanceof Error ? error.message : error); confirmed.value = false }
}
watch(() => JSON.stringify(writeForm), () => { confirmed.value = false })
watch(() => [readForm.address, readForm.type, readForm.length, readForm.encoding, readForm.reverse], () => { values.value = []; trend.value = []; lastReadContext.value = null })
watch(() => writeForm.mode, mode => { if (mode === 'hex') writeForm.increment = false })
watch(() => props.connected, connected => { if (!connected) { runner.stop(); confirmed.value = false } })
onBeforeUnmount(() => runner.stop())
defineExpose({ stop: () => runner.stop() })
</script>

<template>
  <section class="io-workspace" aria-label="单数据读写测试">
    <div v-if="message" class="studio-alert" role="alert">{{ message }}</div>
    <div v-if="running" class="task-banner" role="status"><span class="status-dot online"></span>{{ taskName }}正在运行 · 本页累计 {{ taskName === '定时读取' ? counts.read : counts.write }} 次 <NButton size="small" type="warning" @click="runner.stop()"><template #icon><NIcon :component="PauseOutline" /></template>停止任务</NButton></div>
    <div class="io-columns">
      <section class="panel io-card read-card" aria-labelledby="read-title">
        <div class="studio-card-heading"><span class="studio-icon"><NIcon :component="ArrowDownOutline" :size="20" /></span><div><h2 id="read-title">数据读取</h2><p>READ · 数值、文本与实时观察</p></div><span class="studio-badge">{{ counts.read }} 次</span></div>
        <fieldset :disabled="!editable" class="studio-fieldset">
          <div class="studio-fields address-fields"><label>设备地址<input v-model="readForm.address" aria-label="读取地址" :placeholder="defaultAddress(protocol, readForm.type, inovanceSeries)" /></label><label>数量 / 字节<input v-model.number="readForm.length" type="number" min="1" max="1024" :disabled="readForm.type === 's7_string'" /></label><label class="compact-type-control">类型<select :value="readForm.type" aria-label="读取类型" @change="selectType('read', ($event.target as HTMLSelectElement).value as DataType)"><option v-for="type in quickTypes.filter(type => available.includes(type.value))" :key="type.value" :value="type.value">{{ type.label }}</option></select></label></div>
          <div class="type-chips" aria-label="读取数据类型"><button v-for="type in quickTypes" :key="type.value" type="button" :class="{ selected: readForm.type === type.value }" :disabled="!available.includes(type.value)" :title="available.includes(type.value) ? type.value : '当前协议不支持'" :aria-pressed="readForm.type === type.value" @click="selectType('read', type.value)">{{ type.label }}</button></div>
          <div v-if="readForm.type === 'raw_string'" class="studio-fields"><label>文本编码<select v-model="readForm.encoding"><option v-for="encoding in encodings" :key="encoding">{{ encoding }}</option></select></label><label class="check-label"><input v-model="readForm.reverse" type="checkbox" :disabled="modbus" />字节对交换</label></div>
        </fieldset>
        <div class="result-toolbar"><div class="segmented-control" aria-label="结果进制"><button v-for="mode in (['dec', 'hex', 'bin'] as const)" :key="mode" :class="{ selected: display === mode }" :aria-pressed="display === mode" @click="display = mode">{{ mode === 'dec' ? 'Dec' : mode === 'hex' ? 'Hex' : 'Bit' }}</button></div><label class="result-search"><input v-model="filter" placeholder="搜索结果…" aria-label="搜索读取结果" /></label><button class="text-action" :aria-pressed="curve" @click="curve = !curve"><NIcon :component="PulseOutline" />曲线</button></div>
        <div class="read-output" role="log" aria-label="读取结果"><div v-if="!values.length" class="studio-empty"><span>等待第一组设备数据</span><small>选择类型后读取；不会显示模拟结果。</small></div><div v-for="row in resultLines" :key="row.index" class="value-row"><span>{{ String(row.index).padStart(3, '0') }}</span><code>{{ row.text }}</code></div><p v-if="values.length && !resultLines.length" class="studio-empty">没有匹配结果</p></div>
        <p v-if="lastReadContext" class="studio-hint">最近成功：{{ lastReadContext.address }} · {{ lastReadContext.type }} · {{ lastReadContext.time }}</p>
        <div v-if="curve" class="trend-chart"><svg v-if="trend.length > 1" viewBox="0 0 600 110" role="img" aria-label="最近 120 个样本首元素趋势"><path d="M0 90H600M0 55H600M0 20H600" stroke="#dce7f2" stroke-width="1" /><polyline :points="polyline" fill="none" stroke="#0878ed" stroke-width="2.5" /></svg><p v-else>至少读取两次数值后显示曲线。</p><small>最近 120 次首元素 · 等间距采样展示</small></div>
        <div class="timing-row"><span>耗时 <strong>{{ statistics('read').last }} ms</strong></span><span>Min {{ statistics('read').min }}</span><span>Max {{ statistics('read').max }}</span><span>Avg {{ statistics('read').average }}</span></div>
        <div class="io-actions"><NButton type="primary" :disabled="!ready" @click="perform('read')">读取 {{ readForm.type }}</NButton><NButton secondary :disabled="!ready" @click="perform('read', true)">定时读取</NButton><label class="interval-input"><input v-model.number="readForm.interval" type="number" min="200" max="60000" :disabled="!editable" aria-label="定时读取间隔" />ms</label><label class="check-label"><input v-model="suppressDuplicates" type="checkbox" />屏蔽重复</label></div>
        <p class="studio-hint">异步执行，不阻塞界面；定时请求串行等待，完成后再计间隔。</p>
      </section>

      <section class="panel io-card write-card" aria-labelledby="write-title">
        <div class="studio-card-heading"><span class="studio-icon violet"><NIcon :component="ArrowUpOutline" :size="20" /></span><div><h2 id="write-title">数据写入</h2><p>WRITE · 显式确认，精确控制</p></div><span class="studio-badge">{{ counts.write }} 次</span></div>
        <fieldset :disabled="!editable" class="studio-fieldset">
          <div class="studio-fields address-fields"><label>设备地址<input v-model="writeForm.address" aria-label="写入地址" :placeholder="defaultAddress(protocol, writeForm.type, inovanceSeries)" /></label><label>输入格式<select v-model="writeForm.mode"><option value="values">数值 / 文本</option><option value="hex" :disabled="modbus">HEX 字节</option></select></label><label class="compact-type-control">类型<select :value="writeForm.type" aria-label="写入类型" :disabled="writeForm.mode === 'hex'" @change="selectType('write', ($event.target as HTMLSelectElement).value as DataType)"><option v-for="type in quickTypes.filter(type => available.includes(type.value))" :key="type.value" :value="type.value">{{ type.label }}</option></select></label></div>
          <div class="type-chips" aria-label="写入数据类型"><button v-for="type in quickTypes" :key="type.value" type="button" :class="{ selected: writeForm.type === type.value }" :disabled="!available.includes(type.value) || writeForm.mode === 'hex'" :aria-pressed="writeForm.type === type.value" @click="selectType('write', type.value)">{{ type.label }}</button></div>
          <div v-if="writeForm.type === 'raw_string'" class="studio-fields"><label>文本编码<select v-model="writeForm.encoding"><option v-for="encoding in encodings" :key="encoding">{{ encoding }}</option></select></label><label class="check-label"><input v-model="writeForm.reverse" type="checkbox" :disabled="modbus" />字节对交换</label></div>
          <label class="studio-label write-value-label">写入值<textarea v-model="writeForm.text" aria-label="写入值" rows="5" :placeholder="writeForm.mode === 'hex' ? '01 02 0A FF' : '单值：100 / true\n数组：[1,2,3] · 连续：[1:100] · 重复：[1*100]\n字符串按原文写入'" :disabled="writeForm.increment" spellcheck="false"></textarea></label>
          <div class="dynamic-input"><label class="check-label"><input v-model="writeForm.increment" type="checkbox" :disabled="writeForm.mode === 'hex'" />逐次自增</label><input v-model.number="writeForm.start" type="number" :disabled="!writeForm.increment" aria-label="自增起值" /><span>→</span><input v-model.number="writeForm.end" type="number" :disabled="!writeForm.increment" aria-label="自增终值" /><small>到终值后循环</small></div>
        </fieldset>
        <div class="timing-row"><span>耗时 <strong>{{ statistics('write').last }} ms</strong></span><span>Min {{ statistics('write').min }}</span><span>Max {{ statistics('write').max }}</span><span>Avg {{ statistics('write').average }}</span></div>
        <label class="write-confirmation"><input v-model="confirmed" type="checkbox" :disabled="!ready" />我已核对目标地址及值，确认允许写入 PLC。定时写入将重复执行。</label>
        <div class="io-actions"><NButton type="primary" :disabled="!ready || !confirmed" @click="perform('write')">写入数据</NButton><NButton secondary :disabled="!ready || !confirmed" @click="perform('write', true)">定时写入</NButton><label class="interval-input"><input v-model.number="writeForm.interval" type="number" min="200" max="60000" :disabled="!editable" aria-label="定时写入间隔" />ms</label><label class="interval-input"><input v-model.number="writeForm.count" type="number" min="1" max="10000" :disabled="!editable" aria-label="定时写入次数" />次</label></div>
        <p class="studio-hint" role="status">{{ writeResult || 'Bool 支持 True / False / 0 / 1。写入前校验数值范围，最多 1024 项。' }}</p>
      </section>
    </div>
  </section>
</template>
