<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { NButton } from 'naive-ui'
import type { ConnectRequest, PlcProtocol, ReadRequest, ReadResponse, WriteRequest, WriteResponse } from '../types'
import { defaultAddress, isInovanceProtocol, isOmronProtocol, isReadOnlyAddress } from '../types'
import { batchBytes, batchRequest, batchWriteRequest, formatBatch } from '../batchBuffer'
import type { BatchFormat } from '../batchBuffer'
import { parseHex } from '../workbench'
import { generateRustExample } from '../rustExamples'
import type { RustOperation } from '../rustExamples'

const props = defineProps<{
  protocol: PlcProtocol; ready: boolean; editable: boolean; connected: boolean; configuration: ConnectRequest;
  read: (request: ReadRequest) => Promise<ReadResponse | null>;
  write: (request: WriteRequest) => Promise<WriteResponse | null>;
  runTask: (name: string, job: () => Promise<boolean | void>) => Promise<void>;
}>()
const address = ref(defaultAddress(props.protocol, 'u16', props.configuration.inovance.series)), length = ref(10), rawText = ref('')
const parse = ref(false), format = ref<BatchFormat>('hex'), reverse = ref(false), perLine = ref(10), confirmed = ref(false)
const elapsed = ref<number | null>(null), notice = ref(''), query = ref(''), regex = ref(false), searchBusy = ref(false), searchStatus = ref('')
const selection = ref('—'), selectionIndex = ref(-1), resultArea = ref<HTMLTextAreaElement | null>(null)
const snapshot = ref<{ request: ReadRequest; byteLength: number; config: ConnectRequest } | null>(null)
const operation = ref<RustOperation | null>(null)
let worker: Worker | undefined, timer: ReturnType<typeof setTimeout> | undefined
const requestPreview = computed(() => {
  try { return batchRequest(props.protocol, address.value, length.value) } catch { return null }
})
const readOnly = computed(() => isReadOnlyAddress(props.protocol, address.value))
const units = computed(() => requestPreview.value?.dataType === 'u16' ? '寄存器' : requestPreview.value?.dataType === 'bool' ? '线圈' : '字节')
const interpretationHint = computed(() => isInovanceProtocol(props.protocol) ? '汇川字节数据；MB 按内存字节顺序，数值字序请与主读取核对。' : isOmronProtocol(props.protocol) ? 'FINS 字节数据；大端解析不含数值字序转换。' : units.value === '寄存器' ? '寄存器值按高字节在前展开；非原始线帧。' : '连续数据区读取；解析按大端，交换仅影响显示。')
const display = computed(() => {
  if (!rawText.value.trim()) return { text: '', count: 0, error: '' }
  try {
    const bytes = parseHex(rawText.value)
    return { text: formatBatch(bytes, parse.value ? format.value : 'hex', reverse.value, perLine.value), count: bytes.length, error: '' }
  } catch (cause) { return { text: '', count: 0, error: String(cause) } }
})
const writableView = computed(() => !parse.value && !reverse.value)
const code = computed(() => {
  const request = operation.value ?? (requestPreview.value ? { kind: 'read' as const, request: requestPreview.value } : null)
  return request ? generateRustExample(snapshot.value?.config ?? props.configuration, request).operation : '请输入有效的地址和长度。'
})

async function readBlock() {
  if (!props.ready) return
  notice.value = ''
  try {
    const request = batchRequest(props.protocol, address.value, length.value)
    const config: ConnectRequest = JSON.parse(JSON.stringify(props.configuration))
    rawText.value = ''; snapshot.value = null; confirmed.value = false; elapsed.value = null
    operation.value = { kind: 'read', request }
    await props.runTask('批量读取', async () => {
      const response = await props.read(request)
      if (!response) { notice.value = '读取失败，请检查地址和活动记录。'; return false }
      const bytes = batchBytes(request, response.values)
      rawText.value = formatBatch(bytes, 'hex', false, perLine.value)
      snapshot.value = { request, byteLength: bytes.length, config }
      elapsed.value = response.elapsedMs
    })
  } catch (cause) { notice.value = String(cause) }
}

async function writeBlock() {
  if (!props.ready || !confirmed.value || !snapshot.value) return
  notice.value = ''
  try {
    const request = batchWriteRequest(props.protocol, snapshot.value.request, snapshot.value.byteLength, rawText.value)
    confirmed.value = false; operation.value = { kind: 'write', request }
    await props.runTask('批量回写', async () => {
      const response = await props.write(request)
      notice.value = response ? `已回写 ${response.count} ${response.unit}；请读取核对。` : '回写失败，设备可能已执行部分写入；请核对后再操作。'
      if (response) elapsed.value = response.elapsedMs
      return Boolean(response)
    })
  } catch (cause) { confirmed.value = false; notice.value = String(cause) }
}

function updateSelection() {
  const area = resultArea.value
  selectionIndex.value = area?.selectionStart ?? -1
  selection.value = area && area.selectionEnd > area.selectionStart ? area.value.slice(area.selectionStart, area.selectionEnd).slice(0, 40) : '—'
}
function resetSearch() { worker?.terminate(); clearTimeout(timer); searchBusy.value = false; searchStatus.value = ''; selection.value = '—'; selectionIndex.value = -1 }
function findText() {
  resetSearch()
  if (!query.value) { searchStatus.value = '请输入查找内容'; return }
  searchBusy.value = true
  try {
    worker = new Worker(new URL('../workers/search.ts', import.meta.url), { type: 'module' })
    timer = setTimeout(() => { worker?.terminate(); searchBusy.value = false; searchStatus.value = '查找超时，请简化正则表达式' }, 500)
    worker.onmessage = event => {
      clearTimeout(timer); worker?.terminate(); searchBusy.value = false
      const { indices, error } = event.data
      searchStatus.value = error ? String(error) : indices.length ? `找到 ${indices.length} 处` : '未找到'
      if (indices?.length && resultArea.value) {
        resultArea.value.focus()
        resultArea.value.setSelectionRange(indices[0], indices[0] + (regex.value ? 1 : query.value.length))
        updateSelection()
      }
    }
    worker.onerror = () => { clearTimeout(timer); worker?.terminate(); searchBusy.value = false; searchStatus.value = '查找失败，请重试' }
    worker.postMessage({ text: resultArea.value?.value ?? display.value.text, query: query.value, regex: regex.value })
  } catch (cause) { clearTimeout(timer); worker?.terminate(); searchBusy.value = false; searchStatus.value = String(cause) }
}
watch([address, length, () => props.protocol], () => { rawText.value = ''; snapshot.value = null; operation.value = null; elapsed.value = null; confirmed.value = false; notice.value = ''; resetSearch() })
watch([rawText, parse, format, reverse, perLine], () => { confirmed.value = false; resetSearch() })
watch(perLine, value => { try { if (rawText.value.trim()) rawText.value = formatBatch(parseHex(rawText.value), 'hex', false, value) } catch {} })
watch(() => props.connected, () => { confirmed.value = false; snapshot.value = null })
onBeforeUnmount(resetSearch)
</script>

<template>
  <section class="batch-console" aria-label="连续批量读取">
    <h3 class="sr-only">批量读取</h3>
    <div class="batch-topbar">
      <label>地址<input v-model="address" :disabled="!editable" aria-label="批量读取地址" /></label>
      <label>长度<input v-model.number="length" :disabled="!editable" type="number" min="1" :max="units === '寄存器' ? 512 : 1024" aria-label="批量读取长度" /><small>{{ units }}</small></label>
      <NButton type="primary" :disabled="!ready" @click="readBlock">批量读取</NButton>
    </div>
    <div class="batch-output-row">
      <label class="batch-result-label">结果<textarea ref="resultArea" :value="writableView ? rawText : display.text" :readonly="!writableView || !editable || !snapshot" aria-label="批量读取结果" placeholder="读取后显示 HEX；关闭解析和字节交换可编辑回写内容" spellcheck="false" @input="rawText = ($event.target as HTMLTextAreaElement).value" @select="updateSelection" @keyup="updateSelection" @mouseup="updateSelection"></textarea></label>
      <div class="batch-write-actions"><NButton :disabled="!ready || !snapshot || !confirmed || readOnly || !!display.error" @click="writeBlock">批量回写</NButton><label class="check-label"><input v-model="confirmed" type="checkbox" :disabled="!ready || !snapshot || readOnly || !!display.error" aria-label="确认批量回写" />确认回写原地址</label><small>{{ readOnly ? '只读区域' : '解析不改变回写字节' }}</small></div>
    </div>
    <div class="batch-rust-row"><span>Rust</span><pre class="code-output" tabindex="0" aria-label="批量读取 Rust 代码">{{ code }}</pre></div>
    <div class="batch-parsebar">
      <label class="check-label"><input v-model="parse" type="checkbox" aria-label="批量解析" />解析</label>
      <select v-model="format" aria-label="批量解析类型"><option value="hex">HEX</option><option value="u16">UInt16</option><option value="i16">Int16</option><option value="u32">UInt32</option><option value="i32">Int32</option><option value="f32">Float</option><option value="f64">Double</option><option value="ascii">ASCII</option><option value="utf-8">UTF-8</option><option value="utf-16le">UTF-16 LE</option><option value="utf-16be">UTF-16 BE</option></select>
      <label class="check-label"><input v-model="reverse" type="checkbox" aria-label="批量字节交换" />字节交换</label>
      <span>字节 {{ display.count }}</span><span>耗时 {{ elapsed ?? '—' }} ms</span>
      <label>每行<input v-model.number="perLine" type="number" min="1" max="64" aria-label="批量每行数量" /></label>
      <span class="batch-selection" :title="selection">Select: {{ selection }} · Index: {{ selectionIndex }}</span>
    </div>
    <div class="batch-searchbar"><label>查找<input v-model="query" aria-label="批量查找字符串" placeholder="字符串 / 正则" @keyup.enter="findText" /></label><label class="check-label"><input v-model="regex" type="checkbox" aria-label="批量正则表达式" />正则</label><NButton :loading="searchBusy" @click="findText">查找</NButton><span class="batch-feedback" role="status" :title="display.error || notice || searchStatus || interpretationHint">{{ display.error || notice || searchStatus || interpretationHint }}</span></div>
  </section>
</template>

<style scoped>
.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.batch-console { flex: 1; min-height: 0; display: grid; grid-template-rows: 26px minmax(42px, 1fr) minmax(22px, .4fr) 24px 24px; gap: 4px; }
.batch-console label { display: flex; align-items: center; gap: 5px; min-width: 0; white-space: nowrap; color: #526d8f; font-size: 10px; }
.batch-topbar { display: grid; grid-template-columns: minmax(0, 1fr) minmax(95px, .3fr) auto; gap: 9px; }
.batch-topbar input { min-width: 0; flex: 1; }
.batch-topbar small { font-size: 9px; }
.batch-output-row { min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) 116px; gap: 9px; }
.batch-console .batch-result-label { min-height: 0; align-items: stretch; }
.batch-result-label textarea { flex: 1; min-width: 0; min-height: 24px; resize: none; }
.batch-write-actions { display: flex; flex-direction: column; align-items: stretch; justify-content: center; gap: 4px; }
.batch-write-actions small { font-size: 8px; color: #8190a5; text-align: center; }
.batch-rust-row { display: flex; align-items: stretch; min-height: 0; gap: 6px; font-size: 10px; color: #637a96; }
.batch-rust-row .code-output { min-width: 0; min-height: 0; padding: 3px 6px; margin: 0; font-size: 10px; }
.batch-parsebar, .batch-searchbar { display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 9px; color: #6c839f; }
.batch-parsebar > span { white-space: nowrap; }
.batch-parsebar > select { width: 85px; }
.batch-parsebar input[type='number'] { width: 38px; }
.batch-console input[type='number'] { appearance: textfield; }
.batch-console input[type='number']::-webkit-inner-spin-button, .batch-console input[type='number']::-webkit-outer-spin-button { appearance: none; margin: 0; }
.batch-selection { flex: 1; overflow: hidden; text-overflow: ellipsis; }
.batch-searchbar > label:first-child { width: 30%; min-width: 140px; }
.batch-searchbar input:not([type='checkbox']) { width: 100%; min-width: 0; }
.batch-feedback { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.batch-console :deep(.n-button) { --n-height: 24px !important; --n-padding: 0 9px !important; font-size: 10px; }
@media (min-width: 761px) and (max-height: 700px) {
  .batch-console { grid-template-rows: 23px minmax(38px, 1fr) minmax(20px, .3fr) 21px 21px; gap: 2px; }
  .batch-console label, .batch-rust-row { font-size: 9px; gap: 3px; }
  .batch-console :deep(.n-button) { --n-height: 22px !important; --n-padding: 0 6px !important; font-size: 9px; }
  .batch-write-actions small { display: none; }
  .batch-parsebar, .batch-searchbar { gap: 5px; font-size: 8px; }
  .batch-topbar { gap: 6px; }
}
@media (max-width: 760px) {
  .batch-console { min-height: 420px; grid-template-rows: auto minmax(120px, 1fr) 100px auto auto; }
  .batch-topbar { grid-template-columns: minmax(0, 1fr) 100px; }
  .batch-topbar > :last-child { grid-column: 1 / -1; }
  .batch-output-row { grid-template-columns: minmax(0, 1fr); }
  .batch-write-actions { flex-direction: row; }
  .batch-parsebar, .batch-searchbar { flex-wrap: wrap; }
}
</style>
