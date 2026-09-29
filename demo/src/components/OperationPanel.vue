<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { NAlert, NButton, NCheckbox, NForm, NFormItem, NIcon, NInput, NInputNumber, NSelect, NTab, NTabs, NTag } from 'naive-ui'
import { ArrowDownOutline, ArrowUpOutline, InformationCircleOutline, SwapVerticalOutline } from '@vicons/ionicons5'
import { availableDataTypes, defaultAddress, isModbusProtocol, isReadOnlyModbusAddress } from '../types'
import type { DataType, PlcProtocol, ReadRequest, WriteRequest } from '../types'

const props = defineProps<{ disabled: boolean; busy: boolean; pending: string; protocol: PlcProtocol }>()
const emit = defineEmits<{ read: [request: ReadRequest]; write: [request: WriteRequest] }>()
const form = reactive({ address: 'DB1.0', dataType: 'u16' as DataType, length: 1 as number | null, mode: 'single' as 'single' | 'array', input: '' })
const confirmed = ref(false)
const operation = ref<'read' | 'write'>('read')
const validationError = ref('')
const isString = computed(() => form.dataType === 'raw_string' || form.dataType === 's7_string')
const byteLength = computed(() => new TextEncoder().encode(form.input).length)
const modes = [{ label: '单个值', value: 'single' }, { label: '连续数组', value: 'array' }]
const dataTypes = computed(() => availableDataTypes(props.protocol))
const isModbus = computed(() => isModbusProtocol(props.protocol))
const addressPlaceholder = computed(() => defaultAddress(props.protocol, form.dataType))
const readOnlyAddress = computed(() => isModbus.value && isReadOnlyModbusAddress(form.address))
const help = computed(() => {
  if (isModbus.value && form.dataType === 'raw_string') return 'HR0/IR0 读取原始 UTF-8 文本，数量为字节数，不含长度头。仅 HR 可写；奇数字节写入会先读取末寄存器以保留相邻字节。'
  if (isModbus.value) return form.dataType === 'bool'
    ? 'C0 为线圈，DI0 为只读离散输入；数量为位数，写入使用 true/false。地址从零开始。'
    : 'HR0 为保持寄存器，IR0 为只读输入寄存器。数量按元素计算；u32/f32 占 2 个寄存器，u64/f64 占 4 个。40001 不会自动换算。'
  if (form.dataType === 's7_string') return 'S7 地址指向 STRING 长度头；三菱请选择原始 UTF-8 文本。'
  if (form.dataType === 'raw_string') return '地址指向正文，数量为 UTF-8 字节数。写入不带长度头，不补零或清理尾部。'
  if (props.protocol !== 's7') return form.dataType === 'bool' ? '三菱位地址示例 M100 / X10，数量按位计算；写入使用 true 或 false。' : '三菱字地址示例 D100 / W20，数量按元素计算；32 位数值占用两个字。'
  if (form.dataType === 'bool') return '数量按位计算。支持 DB1.0 / DB1.0.1 / M0.1；写入使用 true 或 false。'
  return '数量按元素计算，非字节数。支持 DB1.0 / DB1.DBW0，元素宽度由数据类型决定。'
})
const placeholder = computed(() => isString.value ? '输入原始文本，无需引号' : form.dataType === 'bool' ? 'true, false, true' : form.mode === 'array' ? '100, 200, 300' : '例如 100')

watch(() => props.protocol, protocol => {
  if (!availableDataTypes(protocol).some(option => option.value === form.dataType)) form.dataType = 'u16'
  form.address = defaultAddress(protocol, form.dataType)
  form.input = ''
  form.mode = 'single'
  operation.value = 'read'
  confirmed.value = false
}, { immediate: true, flush: 'sync' })

watch(() => form.dataType, (type, previous) => {
  if (form.address === defaultAddress(props.protocol, previous)) form.address = defaultAddress(props.protocol, type)
})

watch(() => [form.address, form.dataType, form.mode, form.input, props.disabled, props.busy, operation.value], () => {
  confirmed.value = false
  validationError.value = ''
  if (isString.value) form.mode = 'single'
}, { flush: 'sync' })

function changeOperation(next: 'read' | 'write') {
  if (props.busy) return
  operation.value = next
  document.getElementById('operation-' + next)?.focus()
}

function submit() {
  if (props.disabled) return
  validationError.value = ''
  if (!dataTypes.value.some(option => option.value === form.dataType)) {
    validationError.value = '当前协议不支持此数据类型。'
    return
  }
  const address = form.address.trim()
  if (!address || new TextEncoder().encode(address).length > 128) {
    validationError.value = '请填写有效地址，长度不得超过 128 字节。'
    return
  }
  if (operation.value === 'read') {
    if (form.dataType !== 's7_string' && (form.length === null || !Number.isInteger(form.length) || form.length < 1 || form.length > 1024)) {
      validationError.value = '读取数量必须为 1–1024 的整数。'
      return
    }
    emit('read', { address, dataType: form.dataType, ...(form.dataType === 's7_string' ? {} : { length: form.length! }) })
    return
  }
  if (!confirmed.value) return
  if (readOnlyAddress.value) {
    validationError.value = 'DI/IR 是只读区域，写入请使用 C/HR。'
    return
  }
  const values = isString.value ? [form.input] : form.mode === 'single' ? [form.input.trim()] : form.input.trim().replace(/^\[/, '').replace(/\]$/, '').split(/[,;\s]+/).filter(Boolean)
  if (values.length === 0 || values.length > 1024 || (!isString.value && values.some(value => !value)) || (form.dataType === 'raw_string' && !form.input)) {
    validationError.value = '请填写有效写入值；数组最多 1024 项，原始文本不能为空。'
    return
  }
  emit('write', { address, dataType: form.dataType, mode: isString.value ? 'single' : form.mode, values, confirmed: true })
  confirmed.value = false
}
</script>

<template>
  <section class="panel operation-panel" aria-labelledby="operation-title">
    <div class="panel-heading">
      <span class="panel-icon accent"><NIcon :component="SwapVerticalOutline" :size="21" /></span>
      <div><h2 id="operation-title">数据读写</h2><p>精确到地址，专注于数据。</p></div>
      <NTag class="heading-end" size="small" round :bordered="false">PLC 字序</NTag>
    </div>
    <NTabs v-model:value="operation" class="operation-tabs" type="segment" size="small" role="tablist" aria-label="操作模式" @keydown.left.prevent="changeOperation('read')" @keydown.right.prevent="changeOperation('write')">
      <NTab id="operation-read" name="read" :disabled="busy" role="tab" :tabindex="operation === 'read' ? 0 : -1" :aria-selected="operation === 'read'" aria-controls="operation-form" @keydown.enter="changeOperation('read')" @keydown.space.prevent="changeOperation('read')">读取数据</NTab>
      <NTab id="operation-write" name="write" :disabled="busy" role="tab" :tabindex="operation === 'write' ? 0 : -1" :aria-selected="operation === 'write'" aria-controls="operation-form" @keydown.enter="changeOperation('write')" @keydown.space.prevent="changeOperation('write')">写入数据</NTab>
    </NTabs>
    <NForm id="operation-form" :disabled="busy" label-placement="top" :show-feedback="false" @submit.prevent="submit">
      <div class="form-grid operation-fields">
        <NFormItem label="数据地址"><NInput v-model:value="form.address" :placeholder="addressPlaceholder" :maxlength="128" :input-props="{ spellcheck: false, 'aria-label': '数据地址' }" /></NFormItem>
        <NFormItem label="数据类型"><NSelect v-model:value="form.dataType" :options="dataTypes" aria-label="数据类型" /></NFormItem>
      </div>
      <div id="type-help" class="type-help"><NIcon :component="InformationCircleOutline" :size="15" /><span>{{ help }}</span></div>
      <p v-if="isModbus" class="field-hint">支持 x=2;100、x=2;HR100、x=2;C0：本次使用站号 2，不改变连接默认站号。</p>
      <div v-if="operation === 'read'" class="read-actions">
        <NFormItem v-if="form.dataType !== 's7_string'" :label="form.dataType === 'raw_string' ? '读取字节数' : '读取元素数'"><NInputNumber v-model:value="form.length" :min="1" :max="1024" :precision="0" :input-props="{ 'aria-label': '读取数量', 'aria-describedby': 'type-help' }" /></NFormItem>
        <span v-else class="field-hint">从 STRING 头自动获取长度</span>
        <div><NButton type="primary" attr-type="submit" :disabled="disabled" :loading="pending === '读取'"><template #icon><NIcon :component="ArrowDownOutline" /></template>读取数据</NButton><span class="action-hint">{{ disabled && !busy ? '连接设备后可执行' : '手动读取 · 不自动轮询' }}</span></div>
      </div>
      <div v-else class="write-area">
        <NAlert v-if="readOnlyAddress" type="error">DI/IR 是只读区域，写入请使用 C/HR。</NAlert>
        <div class="write-mode"><span>写入内容</span><NSelect v-if="!isString" v-model:value="form.mode" :options="modes" size="small" aria-label="写入模式" /><NTag v-else size="small" :bordered="false">UTF-8 · {{ byteLength }} 字节</NTag></div>
        <NInput v-model:value="form.input" type="textarea" :autosize="{ minRows: 3, maxRows: 6 }" :placeholder="placeholder" :input-props="{ spellcheck: false, 'aria-label': '写入值' }" />
        <p class="field-hint">{{ isString ? '字符串容量由 Rust 库校验。请先确认 PLC 中的预留内存。' : '数组用逗号、空格或换行分隔。整数使用十进制，64 位整数保留完整精度。' }}</p>
        <NAlert type="warning" title="写入会修改真实 PLC 数据">请核对地址、类型与内存容量。超时不代表未执行；多包写入可能仅完成一部分，请勿直接重试。</NAlert>
        <div class="write-footer"><NCheckbox v-model:checked="confirmed" :disabled="disabled || readOnlyAddress">我已核对本次地址与内容</NCheckbox><NButton type="error" attr-type="submit" :disabled="disabled || !confirmed || readOnlyAddress" :loading="pending === '写入'"><template #icon><NIcon :component="ArrowUpOutline" /></template>确认写入</NButton></div>
      </div>
      <p v-if="validationError" class="validation-error" role="alert">{{ validationError }}</p>
    </NForm>
  </section>
</template>
