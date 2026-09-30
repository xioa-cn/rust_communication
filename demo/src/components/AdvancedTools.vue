<script setup lang="ts">
import {computed, nextTick, onBeforeUnmount, reactive, ref, watch} from 'vue'
import {NButton, NIcon} from 'naive-ui'
import {ConstructOutline, PauseOutline} from '@vicons/ionicons5'
import {availableDataTypes, defaultAddress, isCipProtocol, isModbusProtocol, protocolLabels} from '../types'
import {createDeviceConnection} from '../deviceCatalog'
import type {ConnectRequest, PlcProtocol, ReadRequest, ReadResponse, WriteRequest, WriteResponse} from '../types'
import {
  createSerialRunner,
  decodeText,
  downloadText,
  parseBatch,
  parseHex,
  reversePairs
} from '../workbench'
import type {Point, TextEncoding} from '../workbench'
import {generateRustExample} from '../rustExamples'
import type {RustOperation} from '../rustExamples'
import BatchReadPanel from './BatchReadPanel.vue'
import {addressExamples, addressNotes} from '../addressExamples'

const props = defineProps<{
  protocol: PlcProtocol;
  connected: boolean;
  desktop: boolean;
  busy: boolean;
  locked: boolean;
  connectionConfig: ConnectRequest | null;
  exampleOperation: RustOperation;
  read: (request: ReadRequest) => Promise<ReadResponse | null>;
  write: (request: WriteRequest) => Promise<WriteResponse | null>;
}>()
const emit = defineEmits<{ running: [value: boolean] }>()
const tabs = [
  {id: 'batch', label: '批量读取'}, {id: 'raw', label: '报文读取'}, {id: 'points', label: '点位变量'},
  {id: 'addresses', label: '地址示例'}, {id: 'code', label: '代码示例'},
]
const activeTab = ref('batch'), message = ref(''), running = ref(false), taskName = ref('')
const editable = computed(() => !props.busy && !props.locked)
const ready = computed(() => props.desktop && props.connected && editable.value)
const runner = createSerialRunner(value => {
  running.value = value;
  emit('running', value);
  if (!value) taskName.value = ''
})
const types = computed(() => availableDataTypes(props.protocol))
const modbus = computed(() => isModbusProtocol(props.protocol))
const inovanceSeries = computed(() => props.connectionConfig?.inovance.series ?? 'AM')
const addressRows = computed(() => addressExamples(props.protocol, inovanceSeries.value))
const rawAddress = ref(defaultAddress(props.protocol, 'u16', inovanceSeries.value)), rawLength = ref(10), rawHex = ref(''),
    parseEnabled = ref(true), rawReverse = ref(false), rawMode = ref<TextEncoding | 'hex' | 'u16' | 'f32'>('hex'),
    byteOrder = ref('ABCD'), rawElapsed = ref<number | null>(null)
const search = reactive({query: '', regex: false, result: '', busy: false})
const points = ref<Point[]>([]), pointInterval = ref(1000)
let pointId = 0
let searchWorker: Worker | undefined, searchTimer: ReturnType<typeof setTimeout> | undefined

async function guard(task: () => Promise<void> | void) {
  message.value = ''
  try {
    await task()
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  }
}

async function task(name: string, job: (index: number) => Promise<boolean | void>, interval = 0, count = 1) {
  if (!ready.value) return
  await guard(async () => {
    taskName.value = name;
    await runner.start(job, interval, count)
  })
}


async function readRaw() {
  await task('读取数据区字节', async () => {
    if (!Number.isInteger(rawLength.value) || rawLength.value < 1 || rawLength.value > (modbus.value ? 512 : 1024)) throw new Error('最多读取 1024 字节或 512 个寄存器。')
    const request = parseBatch(`${rawAddress.value},${modbus.value ? 'u16' : 'u8'},${rawLength.value}`, props.protocol)[0]
    const response = await props.read(request)
    if (!response) return false
    const bytes = modbus.value ? response.values.flatMap(value => [Number(value) >> 8, Number(value) & 255]) : response.values.map(Number)
    rawHex.value = bytes.map(value => value.toString(16).padStart(2, '0').toUpperCase()).join(' ')
    rawElapsed.value = response.elapsedMs
  })
}

const parsedRaw = computed(() => {
  if (!rawHex.value.trim()) return {output: '读取数据区，或粘贴 HEX 进行离线解析。', length: 0}
  try {
    let bytes = parseHex(rawHex.value)
    if (rawReverse.value) bytes = reversePairs(bytes)
    if (!parseEnabled.value || rawMode.value === 'hex') return {
      output: [...bytes].map(value => value.toString(16).padStart(2, '0').toUpperCase()).join(' '),
      length: bytes.length
    }
    if (rawMode.value === 'u16' || rawMode.value === 'f32') {
      const width = rawMode.value === 'u16' ? 2 : 4, result: string[] = []
      if (bytes.length % width) throw new Error(`字节数必须为 ${width} 的倍数。`)
      for (let offset = 0; offset < bytes.length; offset += width) {
        let chunk: Uint8Array = bytes.slice(offset, offset + width)
        if (byteOrder.value === 'BADC' || byteOrder.value === 'DCBA') chunk = reversePairs(chunk)
        if (width === 4 && (byteOrder.value === 'CDAB' || byteOrder.value === 'DCBA')) chunk = Uint8Array.from([...chunk.slice(2), ...chunk.slice(0, 2)])
        const view = new DataView(chunk.buffer)
        result.push(String(width === 2 ? view.getUint16(0) : view.getFloat32(0)))
      }
      return {output: result.join(', '), length: bytes.length}
    }
    return {output: decodeText(bytes, rawMode.value), length: bytes.length}
  } catch (error) {
    return {output: String(error), length: 0}
  }
})

function findText() {
  searchWorker?.terminate()
  clearTimeout(searchTimer)
  search.busy = true
  search.result = ''
  try {
    searchWorker = new Worker(new URL('../workers/search.ts', import.meta.url), {type: 'module'})
    const finish = (text: string) => {
      search.result = text;
      search.busy = false;
      clearTimeout(searchTimer);
      searchWorker?.terminate();
      searchWorker = undefined
    }
    searchWorker.onmessage = event => finish(event.data.error || `找到 ${event.data.indices.length} 处；字符 Index：${event.data.indices.slice(0, 30).join(', ') || '-1'}`)
    searchWorker.onerror = () => finish('搜索失败；请检查表达式。')
    searchTimer = setTimeout(() => finish('表达式执行超时，已终止搜索。'), 1000)
    searchWorker.postMessage({text: parsedRaw.value.output, query: search.query, regex: search.regex})
  } catch (error) {
    search.busy = false;
    search.result = String(error)
  }
}


function addPoint() {
  if (points.value.length >= 100) {
    message.value = '点表最多 100 行。';
    return
  }
  points.value.push({
    id: ++pointId,
    name: `变量 ${pointId}`,
    address: defaultAddress(props.protocol, 'u16', inovanceSeries.value),
    dataType: 'u16',
    length: 1,
    value: '—',
    elapsedMs: null,
    error: ''
  })
}

async function samplePoints(repeat = false) {
  await guard(async () => {
    if (!points.value.length) throw new Error('请先添加点位。')
    if (repeat && (!Number.isInteger(pointInterval.value) || pointInterval.value < 200 || pointInterval.value > 60000)) throw new Error('点表间隔为 200–60000 ms。')
    const requests = points.value.map(point => parseBatch(`${point.address},${point.dataType},${point.length}`, props.protocol)[0])
    await task(repeat ? '点表监视' : '读取点表', async () => {
      for (const [index, request] of requests.entries()) {
        if (runner.stopping) break
        const response = await props.read(request), point = points.value[index]
        point.error = response ? '' : '读取失败'
        if (!response) return false
        point.value = response.values.join(', ')
        point.elapsedMs = response.elapsedMs
      }
    }, repeat ? pointInterval.value : 0, repeat ? 0 : 1)
  })
}

async function importPoints(event: Event) {
  const input = event.target as HTMLInputElement, file = input.files?.[0]
  if (!file || !editable.value) return
  await guard(async () => {
    if (file.size > 100000) throw new Error('点表文件不得超过 100 KB。')
    const rows: unknown = JSON.parse(await file.text())
    if (!Array.isArray(rows) || rows.length > 100) throw new Error('需要最多 100 行的 JSON 数组。')
    const imported = rows.map(row => {
      if (!row || typeof row !== 'object' || typeof row.address !== 'string' || typeof row.name !== 'string' || row.name.length > 80) throw new Error('点表格式无效。')
      const request = parseBatch(`${row.address},${row.dataType},${row.length}`, props.protocol)[0]
      return {
        id: ++pointId,
        name: row.name,
        address: request.address,
        dataType: request.dataType,
        length: request.length ?? 1,
        value: '—',
        elapsedMs: null,
        error: ''
      }
    })
    points.value = imported
  })
  input.value = ''
}

const rustConfig = computed(() => props.connectionConfig ?? {
  ...createDeviceConnection(isCipProtocol(props.protocol) ? props.protocol.replace('_', '-') : props.protocol === 's7' ? 's7-1200' : props.protocol === 'inovance_modbus_tcp' ? 'inovance-am' : props.protocol.startsWith('omron_') ? 'omron-fins-tcp' : props.protocol.startsWith('modbus_') ? 'modbus-tcp' : 'melsec-mc'),
  protocol: props.protocol
})
const rustExample = computed(() => generateRustExample(rustConfig.value, props.exampleOperation))

function changeTab(id: string) {
  activeTab.value = id;
  message.value = ''
}

async function handleTabKey(event: KeyboardEvent, id: string) {
  const index = tabs.findIndex(tab => tab.id === id)
  const target = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index - 1 + tabs.length) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1
  if (target < 0) return
  event.preventDefault()
  changeTab(tabs[target].id)
  await nextTick()
  document.getElementById(`tool-tab-${tabs[target].id}`)?.focus()
}

watch(() => props.connected, connected => {
  if (!connected) {
    runner.stop();
  }
})
onBeforeUnmount(() => {
  runner.stop();
  searchWorker?.terminate();
  clearTimeout(searchTimer)
})
defineExpose({stop: () => runner.stop(), showCode: () => changeTab('code')})
</script>

<template>
  <section class="panel advanced-tools" aria-labelledby="advanced-title">
    <div class="studio-card-heading"><span class="studio-icon"><NIcon :component="ConstructOutline" :size="20"/></span>
      <div><h2 id="advanced-title">工程工具箱</h2>
        <p>从批量调试到点表监视，按任务切换。</p></div>
      <span class="studio-badge">{{ tabs.length }} 个工具</span></div>
    <div class="tool-tabs" role="tablist" aria-label="工程工具">
      <button v-for="tab in tabs" :id="`tool-tab-${tab.id}`" :key="tab.id" type="button" role="tab"
              :tabindex="activeTab === tab.id ? 0 : -1" :aria-selected="activeTab === tab.id"
              :aria-controls="`tool-panel-${tab.id}`" :class="{ active: activeTab === tab.id }"
              @click="changeTab(tab.id)" @keydown="handleTabKey($event, tab.id)">{{ tab.label }}
      </button>
    </div>
    <div v-if="message" class="studio-alert" role="alert">{{ message }}</div>
    <div v-if="running" class="task-banner" role="status">{{ taskName }}正在运行
      <NButton type="warning" size="small" @click="runner.stop()">
        <template #icon>
          <NIcon :component="PauseOutline"/>
        </template>
        停止任务
      </NButton>
      <small>停止后等待已发出的请求完成，不再发送下一次。</small></div>
    <div :id="`tool-panel-${activeTab}`" class="tool-body" role="tabpanel" :aria-labelledby="`tool-tab-${activeTab}`">
      <KeepAlive><BatchReadPanel v-if="activeTab === 'batch'" :protocol="protocol" :ready="ready" :editable="editable"
                      :connected="connected" :configuration="rustConfig" :read="read" :write="write" :run-task="task"/></KeepAlive>

      <template v-if="activeTab === 'raw'">
        <div class="tool-description"><h3>数据区字节与报文解析</h3>
          <p>{{
              modbus ? '读取 u16 寄存器值并按高字节在前展开；不是网络线帧，也不保留原始 MBAP/CRC。' : '读取地址对应的原始数据字节，不包含协议头；不是网络抓包。'
            }} 可粘贴 HEX 离线解析。</p></div>
        <div class="tool-fields"><label>地址<input v-model="rawAddress" :disabled="!editable"/></label><label>{{
            modbus ? '寄存器数量' : '字节数量'
          }}<input v-model.number="rawLength" :disabled="!editable" type="number" min="1"
                   :max="modbus ? 512 : 1024"/></label>
          <NButton type="primary" :disabled="!ready" @click="readRaw">读取字节</NButton>
        </div>
        <label class="studio-label">HEX 内容<textarea v-model="rawHex" :disabled="!editable" rows="3" spellcheck="false"
                                                      aria-label="HEX 内容"
                                                      placeholder="例如：00 01 00 02 41 42"></textarea></label>
        <div class="tool-fields"><label class="check-label"><input v-model="parseEnabled"
                                                                   type="checkbox"/>解析</label><label>解释类型<select
            v-model="rawMode">
          <option value="hex">HEX</option>
          <option value="ascii">ASCII</option>
          <option value="utf-8">UTF-8</option>
          <option value="utf-16le">UTF-16 LE</option>
          <option value="utf-16be">UTF-16 BE</option>
          <option value="u16">UInt16</option>
          <option value="f32">Float32</option>
        </select></label><label>数值字节序<select v-model="byteOrder">
          <option>ABCD</option>
          <option>BADC</option>
          <option>CDAB</option>
          <option>DCBA</option>
        </select></label><label class="check-label"><input v-model="rawReverse" type="checkbox"/>字节对交换</label><span
            class="studio-hint">{{ parsedRaw.length }} 字节 · {{ rawElapsed ?? '—' }} ms</span></div>
        <pre class="code-output">{{ parsedRaw.output }}</pre>
        <div class="tool-fields"><label>查找字符串<input v-model="search.query"
                                                         placeholder="输入内容或正则表达式"/></label><label
            class="check-label"><input v-model="search.regex" type="checkbox"/>正则表达式</label>
          <NButton :loading="search.busy" @click="findText">查找</NButton>
          <span class="studio-hint" role="status">{{ search.result }}</span></div>
      </template>


      <template v-else-if="activeTab === 'points'">
        <div class="tool-description"><h3>点位变量表</h3>
          <p>为地址命名，按行配置类型和长度。监视串行读取，失败即停；本次页面内保存，可导入导出 JSON。</p></div>
        <div class="tool-actions">
          <NButton :disabled="!editable" @click="addPoint">＋ 添加点位</NButton>
          <NButton type="primary" :disabled="!ready || !points.length" @click="samplePoints()">读取点表</NButton>
          <NButton :disabled="!ready || !points.length" @click="samplePoints(true)">开始监视</NButton>
          <label class="interval-input"><input v-model.number="pointInterval" type="number" min="200" max="60000"
                                               :disabled="!editable" aria-label="点表监视间隔"/>ms</label><label
            class="file-button">导入 JSON<input type="file" accept=".json,application/json" :disabled="!editable"
                                                @change="importPoints"/></label>
          <NButton :disabled="!points.length"
                   @click="downloadText('plc-points.json', JSON.stringify(points.map(({ name, address, dataType, length }) => ({ name, address, dataType, length })), null, 2), 'application/json')">
            导出点表
          </NButton>
        </div>
        <div class="table-scroll">
          <table class="studio-table point-table">
            <thead>
            <tr>
              <th>名称</th>
              <th>地址</th>
              <th>类型</th>
              <th>数量</th>
              <th>当前值</th>
              <th>ms</th>
              <th></th>
            </tr>
            </thead>
            <tbody>
            <tr v-for="point in points" :key="point.id">
              <td><input v-model="point.name" :disabled="!editable" maxlength="80" aria-label="点位名称"/></td>
              <td><input v-model="point.address" :disabled="!editable" aria-label="点位地址"/></td>
              <td><select v-model="point.dataType" :disabled="!editable" aria-label="点位类型">
                <option v-for="type in types" :key="type.value" :value="type.value">{{ type.value }}</option>
              </select></td>
              <td><input v-model.number="point.length" :disabled="!editable" type="number" min="1" max="1024"
                         aria-label="点位数量"/></td>
              <td><code>{{ point.error || point.value }}</code></td>
              <td>{{ point.elapsedMs ?? '—' }}</td>
              <td>
                <button type="button" class="text-action" :disabled="!editable" :aria-label="`删除 ${point.name}`"
                        @click="points = points.filter(row => row.id !== point.id)">删除
                </button>
              </td>
            </tr>
            <tr v-if="!points.length">
              <td colspan="7" class="table-empty">添加第一个变量，建立自己的设备点表。</td>
            </tr>
            </tbody>
          </table>
        </div>
      </template>


      <template v-else-if="activeTab === 'addresses'">
        <div class="tool-description"><h3>{{ protocolLabels[protocol] }} · 地址示例</h3>
          <p>按当前协议列出可直接用于读取与写入的地址格式。示例不会自动连接设备，具体范围仍以 PLC 组态为准。</p></div>
        <div class="table-scroll address-example-scroll">
          <table class="studio-table address-example-table">
            <thead>
            <tr>
              <th>地址类型</th>
              <th>描述信息</th>
              <th>位</th>
              <th>字</th>
              <th>备注</th>
            </tr>
            </thead>
            <tbody>
            <tr v-for="row in addressRows" :key="row.address + row.description">
              <td><code>{{ row.address }}</code></td>
              <td>{{ row.description }}</td>
              <td class="address-mark">{{ row.bit ? '√' : '—' }}</td>
              <td class="address-mark">{{ row.word ? '√' : '—' }}</td>
              <td>{{ row.note }}</td>
            </tr>
            </tbody>
          </table>
        </div>
        <p class="studio-hint">{{ addressNotes(protocol, inovanceSeries) }}</p>
      </template>

      <template v-else-if="activeTab === 'code'">
        <div class="rust-example-toolbar tool-actions"><h3>Rust · {{ rustExample.title }}</h3>
          <NButton @click="downloadText('plc-example.rs', rustExample.source, 'text/plain;charset=utf-8')">保存完整
            .rs
          </NButton>
        </div>
        <div class="rust-example-columns">
          <section><h4>创建 PLC · 当前连接参数</h4>
            <pre class="code-output" tabindex="0" aria-label="创建 PLC Rust 示例">{{ rustExample.creation }}</pre>
          </section>
          <section><h4>{{ rustExample.title }}</h4>
            <pre class="code-output" tabindex="0" aria-label="当前操作 Rust 示例">{{ rustExample.operation }}</pre>
          </section>
        </div>
        <p class="studio-hint">{{ rustExample.dependency }} · 两栏共用 plc；完整 .rs 包含
          main、连接和错误处理。写入默认不确认。</p>
      </template>
    </div>
  </section>
</template>
