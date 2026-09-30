<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { NButton, NCollapse, NCollapseItem, NForm, NFormItem, NIcon, NInput, NInputNumber, NSelect } from 'naive-ui'
import type { FormInst, FormRules } from 'naive-ui'
import { ArrowForwardOutline, HardwareChipOutline, LinkOutline } from '@vicons/ionicons5'
import type { ConnectRequest, ConnectionStatus, PlcProtocol } from '../types'
import { defaultCipOptions, isCipProtocol, isInovanceProtocol, isModbusProtocol, isOmronProtocol, isSerialProtocol, protocolLabels } from '../types'
import { parseCipRoute, validateCipOptions } from '../cip'
import { createDeviceConnection, getDeviceProfile } from '../deviceCatalog'

const props = defineProps<{ deviceId: string; connection: ConnectionStatus; busy: boolean; pending: string; desktop: boolean }>()
const emit = defineEmits<{ connect: [request: ConnectRequest]; disconnect: []; protocolChange: [protocol: PlcProtocol]; configurationChange: [request: ConnectRequest] }>()
const device = computed(() => getDeviceProfile(props.deviceId))
const form = reactive<ConnectRequest>(createDeviceConnection(props.deviceId))
const drafts = new Map<string, ConnectRequest>()
const formRef = ref<FormInst | null>(null)
const isS7 = computed(() => form.protocol === 's7')
const isCip = computed(() => isCipProtocol(form.protocol))
const cipRouteText = ref('')
const cipModes = [{ label: 'UCMM · 非连接消息', value: 'ucmm' }, { label: 'Class 3 · 连接消息', value: 'connected' }]
const cipMode = computed({
  get: () => form.cip.connected ? 'connected' : 'ucmm',
  set: (value: string) => { form.cip.connected = value === 'connected'; form.cip.connectionSize = form.cip.connected ? 1996 : 500 },
})
watch(() => form.cip.route, route => { cipRouteText.value = route.join(', ') }, { immediate: true })
watch(cipRouteText, text => {
  try {
    const route = parseCipRoute(text)
    if (route.join(',') !== form.cip.route.join(',')) form.cip.route = route
  } catch { }
})
const isModbus = computed(() => isModbusProtocol(form.protocol))
const isOmron = computed(() => isOmronProtocol(form.protocol))
const isInovance = computed(() => isInovanceProtocol(form.protocol))
const inovanceModels = computed(() => (device.value.inovanceSeries ?? []).map(series => ({ label: series, value: series })))
const isSerial = computed(() => isSerialProtocol(form.protocol))
const isUdp = computed(() => ['mc_udp_binary', 'mc_udp_ascii', 'modbus_udp', 'omron_fins_udp'].includes(form.protocol))
const protocolHint = computed(() => {
  if (form.protocol === 'omron_cip') return 'EtherNet/IP 默认端口 44818；支持符号标签、原子类型数组及 STRING（0x00D0）读取。字符串数组逐元素读取；不支持 STRING 写入、UDT 和打包 BOOL 数组。'
  if (isCip.value) return 'EtherNet/IP 默认端口 44818；使用符号标签，支持原子类型及数组，不支持 STRING/UDT 和打包 BOOL 数组。连接模式与报文大小须由目标 PLC 支持。'
  if (form.protocol === 's7') return 'S7 默认端口 102；机架、槽位和 TSAP 由 S7 连接组态决定。'
  if (isOmron.value) return form.protocol === 'omron_fins_tcp'
    ? '节点 0 表示自动协商；默认端口 9600、字节序 CDAB。仅支持 IPv4，跨网络请配置路由。'
    : '节点 0 按本地/目标 IPv4 末段推导；默认端口 9600。UDP 就绪不代表 PLC 在线。'
  if (isInovance.value) return '内置 Modbus TCP，默认端口 502、站号 1、字序 CDAB。型号决定地址规则；不是 EasyNet 或 EtherNet/IP。'
  if (isSerial.value) return '串口参数必须与设备一致；站号 1–247，不支持广播。仅在点击连接时打开串口，不自动扫描设备。'
  if (form.protocol === 'modbus_tcp') return 'Modbus TCP 默认端口 502；站号 1–255。字节序按设备手册设置；地址从零开始。'
  if (form.protocol === 'modbus_udp') return '使用 MBAP + PDU 数据报，不是 RTU-over-UDP。请确认设备帧格式与端口；本地 Socket 就绪不代表从站在线。'
  if (isUdp.value) return 'MC UDP 默认端口 6000；UDP 不建立面向连接的 PLC 会话。'
  if (form.protocol === 'a1e_binary' || form.protocol === 'a1e_ascii') return 'A1E 默认端口 6000；主要使用 PC 号，其他 MC 路由字段通常保持默认。'
  return 'MC 默认端口 6000；网络号、PC 号、目标 I/O 和站号必须与 PLC 以太网参数一致。'
})
const protocols = computed(() => device.value.protocols.map(protocol => ({ label: protocolLabels[protocol], value: protocol })))
const byteOrders = [
  { label: 'ABCD · 高字在前', value: 'ABCD' }, { label: 'BADC · 字内换字节', value: 'BADC' },
  { label: 'CDAB · 低字在前', value: 'CDAB' }, { label: 'DCBA · 完全反序', value: 'DCBA' },
]
const parities = [{ label: '偶校验 · Even', value: 'even' }, { label: '奇校验 · Odd', value: 'odd' }, { label: '无校验 · None', value: 'none' }]
const serialDataBits = computed(() => form.protocol === 'modbus_rtu' ? [{ label: '8 位', value: 8 }] : [{ label: '7 位', value: 7 }, { label: '8 位', value: 8 }])
const stopBits = [{ label: '1 位', value: 1 }, { label: '2 位', value: 2 }]
const omronRouteFields = [
  { key: 'sourceNetwork', label: '源网络 · SNA', max: 127 },
  { key: 'destinationNetwork', label: '目标网络 · DNA', max: 127 },
  { key: 'sourceUnit', label: '源单元 · SA2', max: 255 },
  { key: 'destinationUnit', label: '目标单元 · DA2', max: 255 },
  { key: 'gatewayCount', label: '网关跳数 · GCT', max: 7 },
] as const
const rules: FormRules = {
  host: { required: true, whitespace: true, trigger: ['input', 'blur'], validator: (_rule, value: string) => {
    if (!value?.trim()) return new Error('请输入设备 IP 地址')
    if (!isOmron.value) return true
    const octets = value.trim().split('.')
    return octets.length === 4 && octets.every(part => /^(?:0|[1-9]\d{0,2})$/.test(part) && Number(part) <= 255)
      && !['0.0.0.0', '255.255.255.255'].includes(octets.map(Number).join('.'))
      && !(Number(octets[0]) >= 224 && Number(octets[0]) <= 239) ? true : new Error('FINS 需要单播 IPv4 地址')
  } },
  port: { type: 'integer', required: true, min: 1, max: 65535, message: '端口范围 1–65535', trigger: ['change', 'blur'] },
  rack: { type: 'integer', required: true, min: 0, max: 7, message: 'Rack 范围 0–7', trigger: ['change', 'blur'] },
  slot: { type: 'integer', required: true, min: 0, max: 31, message: 'Slot 范围 0–31', trigger: ['change', 'blur'] },
  connectTimeoutMs: { type: 'integer', required: true, min: 1, max: 60000, message: '范围 1–60000 ms', trigger: ['change', 'blur'] },
  receiveTimeoutMs: { type: 'integer', required: true, min: 1, max: 60000, message: '范围 1–60000 ms', trigger: ['change', 'blur'] },
  'inovance.unitId': { type: 'integer', required: true, min: 1, max: 255, message: '站号范围 1–255，不支持广播', trigger: ['change', 'blur'] },
  'modbus.unitId': { type: 'integer', required: true, min: 1, max: 255, message: '站号必须是 1–255 的整数；串口最大 247', trigger: ['change', 'blur'] },
  'omron.sourceNode': { type: 'integer', required: true, min: 0, max: 254, message: '节点范围 0–254，0 为自动', trigger: ['change', 'blur'] },
  'omron.destinationNode': { type: 'integer', required: true, min: 0, max: 254, message: '节点范围 0–254，0 为自动', trigger: ['change', 'blur'] },
  ...Object.fromEntries(omronRouteFields.map(field => [`omron.${field.key}`, { type: 'integer', required: true, min: 0, max: field.max, message: `范围 0–${field.max}`, trigger: ['change', 'blur'] }])),
  'serial.path': { required: true, whitespace: true, max: 256, message: '请输入串口名，例如 COM3', trigger: ['input', 'blur'] },
  'serial.baudRate': { type: 'integer', required: true, min: 1, max: 4000000, message: '波特率必须为 1–4000000 的整数', trigger: ['change', 'blur'] },
}
const validationError = ref('')

watch(() => form.protocol, protocol => {
  emit('protocolChange', protocol)
  validationError.value = ''
  formRef.value?.restoreValidation()
  if (isCipProtocol(protocol)) {
    form.port = 44818
    form.cip = defaultCipOptions(protocol)
    form.localTsap = ''
    form.remoteTsap = ''
    return
  }
  if (isOmronProtocol(protocol)) {
    form.port = 9600
    form.localTsap = ''
    form.remoteTsap = ''
    return
  }
  if (isModbusProtocol(protocol) || isInovanceProtocol(protocol)) {
    form.port = 502
    form.localTsap = ''
    form.remoteTsap = ''
    if (isSerialProtocol(protocol)) {
      form.serial.dataBits = protocol === 'modbus_rtu' ? 8 : 7
      form.receiveTimeoutMs = 1000
      if (form.modbus.unitId > 247) form.modbus.unitId = 1
    }
    return
  }
  if (protocol === 's7') {
    form.port = 102
    form.rack = 0
    form.slot = 1
    form.localTsap = ''
    form.remoteTsap = ''
  } else {
    form.port = 6000
    form.rack = 0
    form.slot = 0
    form.localTsap = ''
    form.remoteTsap = ''
    form.melsec.networkNumber = 0
    form.melsec.pcNumber = 255
    form.melsec.ioNumber = 0x03ff
    form.melsec.stationNumber = 0
    form.melsec.monitoringTimer = 16
  }
}, { immediate: true, flush: 'sync' })

watch(() => form.serial.parity, parity => {
  form.serial.stopBits = parity === 'none' ? 2 : 1
}, { flush: 'sync' })

watch(() => props.deviceId, (id, previousId) => {
  drafts.set(previousId, { ...form, melsec: { ...form.melsec }, modbus: { ...form.modbus }, omron: { ...form.omron }, inovance: { ...form.inovance }, cip: { ...form.cip, route: [...form.cip.route] }, serial: { ...form.serial } })
  const next = drafts.get(id) ?? createDeviceConnection(id)
  const stopBits = next.serial.stopBits
  form.protocol = next.protocol
  Object.assign(form, next)
  form.serial.stopBits = stopBits
  validationError.value = ''
  formRef.value?.restoreValidation()
})

watch([() => props.connection, () => props.deviceId], ([status]) => {
  if (status.connected && status.protocol && device.value.protocols.includes(status.protocol)) form.protocol = status.protocol
  if (status.connected && status.inovanceSeries && device.value.inovanceSeries?.includes(status.inovanceSeries)) form.inovance.series = status.inovanceSeries
}, { immediate: true })

async function connect() {
  if (!props.desktop || props.busy || props.connection.connected || !formRef.value) return
  validationError.value = ''
  try { await formRef.value.validate() } catch { return }
  if (isCip.value) {
    try { form.cip.route = parseCipRoute(cipRouteText.value); validateCipOptions(form.cip) }
    catch (error) { validationError.value = error instanceof Error ? error.message : String(error); return }
  }
  if (isOmron.value && form.omron.destinationNetwork !== 0 && form.omron.destinationNode === 0) {
    validationError.value = '跨网络 FINS 通讯必须显式填写目标节点 DA1（1–254）。'
    return
  }
  if (isInovance.value && !device.value.inovanceSeries?.includes(form.inovance.series)) {
    validationError.value = '请选择当前汇川分类支持的型号。'
    return
  }
  if (isSerial.value && form.modbus.unitId > 247) {
    validationError.value = 'RTU/ASCII 串口站号范围为 1–247。'
    return
  }
  const localTsap = form.localTsap?.trim() ?? ''
  const remoteTsap = form.remoteTsap?.trim() ?? ''
  if (isS7.value && (Boolean(localTsap) !== Boolean(remoteTsap) || [localTsap, remoteTsap].some(value => value && !/^(0[xX])?[0-9a-fA-F]{1,4}$/.test(value)))) {
    validationError.value = 'S7 的 TSAP 请同时填写 1–4 位十六进制数，或同时留空。'
    return
  }
  emit('connect', { ...form, host: form.host.trim(), port: form.port, localTsap, remoteTsap, melsec: { ...form.melsec }, modbus: { ...form.modbus }, omron: { ...form.omron }, inovance: { ...form.inovance }, cip: { ...form.cip, route: [...form.cip.route] }, serial: { ...form.serial, path: form.serial.path.trim() } })
}
watch(form, () => emit('configurationChange', { ...form, melsec: { ...form.melsec }, modbus: { ...form.modbus }, omron: { ...form.omron }, inovance: { ...form.inovance }, cip: { ...form.cip, route: [...form.cip.route] }, serial: { ...form.serial } }), { deep: true, immediate: true })
</script>

<template>
  <section class="connection-panel" :class="{ 'omron-connection': isOmron }" aria-labelledby="connection-title">
    <NForm id="plc-connection-form" ref="formRef" :model="form" :rules="rules" :disabled="busy || connection.connected" label-placement="top" :show-require-mark="false" @submit.prevent="connect">
      <div class="connection-context">
        <div class="connection-device"><span class="connection-device-icon" aria-hidden="true"><NIcon :component="HardwareChipOutline" :size="20" /></span><h2 id="connection-title">{{ device.label }}</h2></div>
        <NFormItem v-if="protocols.length > 1" class="connection-protocol" label="通讯方式" label-placement="left" path="protocol" :show-feedback="false"><NSelect v-model:value="form.protocol" :options="protocols" aria-label="当前分类的通讯方式" /></NFormItem>
        <span v-else class="connection-protocol-label">{{ protocolLabels[form.protocol] }}</span>
      </div>

      <section class="connection-section" aria-labelledby="connection-address-title">
        <h3 id="connection-address-title">{{ isSerial ? '串口设置' : '通讯地址' }}</h3>
        <div v-if="!isSerial" class="connection-grid connection-address-grid">
          <NFormItem label="IP 地址" path="host"><NInput v-model:value="form.host" placeholder="192.168.0.1" :input-props="{ spellcheck: false, 'aria-label': 'IP 地址' }" /></NFormItem>
          <NFormItem label="端口" path="port"><NInputNumber :key="form.protocol" v-model:value="form.port" :min="1" :max="65535" :precision="0" :show-button="false" :input-props="{ 'aria-label': isCip ? 'EtherNetIP 端口，默认 44818' : isInovance ? '汇川端口，默认 502' : isModbus ? 'Modbus 端口，默认 502' : isOmron ? 'FINS 端口，默认 9600' : isS7 ? 'S7 端口，默认 102' : '三菱端口，默认 6000' }" /></NFormItem>
        </div>
        <div v-else class="connection-grid connection-grid-three">
          <NFormItem label="串口名" path="serial.path"><NInput v-model:value="form.serial.path" placeholder="COM3 / /dev/ttyUSB0" :input-props="{ 'aria-label': '串口名', spellcheck: false }" /></NFormItem>
          <NFormItem label="波特率" path="serial.baudRate"><NInputNumber v-model:value="form.serial.baudRate" :min="1" :max="4000000" :precision="0" :show-button="false" :input-props="{ 'aria-label': '波特率' }" /></NFormItem>
          <NFormItem label="数据位"><NSelect v-model:value="form.serial.dataBits" :options="serialDataBits" aria-label="数据位" /></NFormItem>
          <NFormItem label="校验位"><NSelect v-model:value="form.serial.parity" :options="parities" aria-label="校验位" /></NFormItem>
          <NFormItem label="停止位"><NSelect v-model:value="form.serial.stopBits" :options="stopBits" aria-label="停止位" /></NFormItem>
          <NFormItem label="收发超时 / ms" path="receiveTimeoutMs"><NInputNumber v-model:value="form.receiveTimeoutMs" :min="1" :max="60000" :precision="0" :show-button="false" :input-props="{ 'aria-label': '串口收发超时' }" /></NFormItem>
        </div>
      </section>

      <section class="connection-section" aria-labelledby="connection-parameters-title">
        <h3 id="connection-parameters-title">协议参数</h3>
        <div v-if="isS7" class="connection-grid">
          <NFormItem label="机架 · Rack" path="rack"><NInputNumber v-model:value="form.rack" :min="0" :max="7" :precision="0" :input-props="{ 'aria-label': '机架 Rack' }" /></NFormItem>
          <NFormItem label="槽位 · Slot" path="slot"><NInputNumber v-model:value="form.slot" :min="0" :max="31" :precision="0" :input-props="{ 'aria-label': '槽位 Slot' }" /></NFormItem>
        </div>
        <div v-else-if="isModbus" class="connection-grid">
          <NFormItem label="站号 · Unit ID" path="modbus.unitId"><NInputNumber v-model:value="form.modbus.unitId" :min="1" :max="isSerial ? 247 : 255" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'Modbus 站号' }" /></NFormItem>
          <NFormItem label="数值字节序"><NSelect v-model:value="form.modbus.byteOrder" :options="byteOrders" aria-label="Modbus 字节序" /></NFormItem>
        </div>
        <div v-else-if="isInovance" class="connection-grid connection-grid-three">
          <NFormItem label="汇川型号"><NSelect v-model:value="form.inovance.series" :options="inovanceModels" :disabled="busy || connection.connected || inovanceModels.length === 1" aria-label="汇川型号" /></NFormItem>
          <NFormItem label="站号 · Unit ID" path="inovance.unitId"><NInputNumber v-model:value="form.inovance.unitId" :min="1" :max="255" :precision="0" :show-button="false" :input-props="{ 'aria-label': '汇川站号' }" /></NFormItem>
          <NFormItem label="数值字节序"><NSelect v-model:value="form.inovance.byteOrder" :options="byteOrders" aria-label="汇川字节序" /></NFormItem>
        </div>
        <div v-else-if="isCip" class="connection-grid">
          <NFormItem label="连接模式"><NSelect v-model:value="cipMode" :options="cipModes" aria-label="CIP 连接模式" /></NFormItem>
          <NFormItem label="报文大小 / bytes"><NInputNumber v-model:value="form.cip.connectionSize" :min="128" :max="form.cip.connected ? 4000 : 504" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'CIP 报文大小' }" /></NFormItem>
          <NFormItem label="路由字节 · 留空直连"><NInput v-model:value="cipRouteText" placeholder="1, 0（背板槽位 0）" :input-props="{ 'aria-label': 'CIP 路由字节', spellcheck: false }" /></NFormItem>
        </div>
        <div v-else-if="isOmron" class="connection-grid connection-grid-three">
          <NFormItem label="源节点 · SA1" path="omron.sourceNode"><NInputNumber v-model:value="form.omron.sourceNode" :min="0" :max="254" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'FINS 源节点 SA1' }" /></NFormItem>
          <NFormItem label="目标节点 · DA1" path="omron.destinationNode"><NInputNumber v-model:value="form.omron.destinationNode" :min="0" :max="254" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'FINS 目标节点 DA1' }" /></NFormItem>
          <NFormItem label="数据字节序"><NSelect v-model:value="form.omron.byteOrder" :options="byteOrders" aria-label="FINS 字节序" /></NFormItem>
        </div>
        <div v-else :key="`melsec-${form.protocol}`" class="connection-grid connection-grid-four">
          <NFormItem label="网络号"><NInputNumber v-model:value="form.melsec.networkNumber" :min="0" :max="255" :precision="0" :show-button="false" :input-props="{ 'aria-label': '网络号' }" /></NFormItem>
          <NFormItem label="PC 号"><NInputNumber v-model:value="form.melsec.pcNumber" :min="0" :max="255" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'PC 号' }" /></NFormItem>
          <NFormItem label="目标 I/O"><NInputNumber v-model:value="form.melsec.ioNumber" :min="0" :max="65535" :precision="0" :show-button="false" :input-props="{ 'aria-label': '目标 I/O' }" /></NFormItem>
          <NFormItem label="站号"><NInputNumber v-model:value="form.melsec.stationNumber" :min="0" :max="255" :precision="0" :show-button="false" :input-props="{ 'aria-label': '三菱站号' }" /></NFormItem>
        </div>
        <p class="connection-hint"><template v-if="!isS7 && !isModbus && !isOmron && !isInovance">目标 I/O 默认 1023（0x03FF）。</template>{{ protocolHint }}</p>
      </section>

      <NCollapse v-if="!isSerial" class="advanced-settings" :default-expanded-names="[]">
        <NCollapseItem title="高级设置" name="advanced">
          <template #header-extra><span class="connection-advanced-summary">{{ isS7 ? '超时 / TSAP' : isOmron ? '超时 / FINS 路由' : isModbus || isInovance ? '超时设置' : '超时 / 监视定时器' }}</span></template>
          <div class="connection-grid" :class="{ 'connection-grid-four': isS7 || isOmron, 'connection-grid-three': !isS7 && !isModbus && !isOmron && !isInovance }">
            <NFormItem label="连接超时 / ms" path="connectTimeoutMs"><NInputNumber v-model:value="form.connectTimeoutMs" :min="1" :max="60000" :precision="0" :show-button="false" :input-props="{ 'aria-label': '连接超时' }" /></NFormItem>
            <NFormItem label="收发超时 / ms" path="receiveTimeoutMs"><NInputNumber v-model:value="form.receiveTimeoutMs" :min="1" :max="60000" :precision="0" :show-button="false" :input-props="{ 'aria-label': '收发超时' }" /></NFormItem>
            <template v-if="isS7">
              <NFormItem label="本地 TSAP"><NInput v-model:value="form.localTsap" placeholder="0100" :input-props="{ 'aria-label': '本地 TSAP' }" /></NFormItem>
              <NFormItem label="远端 TSAP"><NInput v-model:value="form.remoteTsap" placeholder="0301" :input-props="{ 'aria-label': '远端 TSAP' }" /></NFormItem>
            </template>
            <template v-else-if="isOmron">
              <NFormItem v-for="field in omronRouteFields" :key="field.key" :label="field.label" :path="`omron.${field.key}`"><NInputNumber v-model:value="form.omron[field.key]" :min="0" :max="field.max" :precision="0" :show-button="false" :input-props="{ 'aria-label': `FINS ${field.label}` }" /></NFormItem>
            </template>
            <template v-else-if="isCip">
              <NFormItem label="RPI / μs"><NInputNumber v-model:value="form.cip.packetIntervalUs" :min="1" :max="4294967295" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'CIP RPI' }" /></NFormItem>
              <NFormItem label="连接超时倍率"><NInputNumber v-model:value="form.cip.timeoutMultiplier" :min="0" :max="7" :precision="0" :show-button="false" :input-props="{ 'aria-label': 'CIP 超时倍率' }" /></NFormItem>
            </template>
            <NFormItem v-else-if="!isModbus && !isInovance" label="监视定时器"><NInputNumber v-model:value="form.melsec.monitoringTimer" :min="0" :max="65535" :precision="0" :show-button="false" :input-props="{ 'aria-label': '监视定时器' }" /></NFormItem>
          </div>
          <p v-if="isS7" class="connection-hint">TSAP 为十六进制；同时留空使用型号默认值。</p>
        </NCollapseItem>
      </NCollapse>
      <p v-if="validationError" class="validation-error" role="alert">{{ validationError }}</p>
    </NForm>

    <footer class="connection-footer">
      <div class="connection-session-state" role="status"><p><span class="status-dot" :class="{ online: connection.connected }" aria-hidden="true"></span>{{ connection.connected ? '会话已建立 · 参数已锁定' : desktop ? '等待手动连接' : '浏览器预览 · 连接不可用' }}</p><small>本地连接状态，不代表远端实时在线。</small></div>
      <NButton v-if="connection.connected || pending === '断开'" class="disconnect-action" size="large" secondary :disabled="busy || !connection.connected || !desktop" :loading="pending === '断开'" @click="emit('disconnect')"><template #icon><NIcon :component="LinkOutline" /></template>断开连接</NButton>
      <NButton v-else class="connect-action" type="primary" size="large" attr-type="submit" form="plc-connection-form" :disabled="busy || !desktop" :loading="pending === '连接'"><template #icon><NIcon :component="ArrowForwardOutline" /></template>建立连接</NButton>
    </footer>
  </section>
</template>
