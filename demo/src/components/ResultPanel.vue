<script setup lang="ts">
import { computed, h, ref, watch } from 'vue'
import { NButton, NDataTable, NIcon, NSpin, NTag } from 'naive-ui'
import type { DataTableColumns } from 'naive-ui'
import { CheckmarkOutline, CopyOutline, FileTrayOutline, ListOutline } from '@vicons/ionicons5'
import type { ReadResult } from '../types'

const props = defineProps<{ result: ReadResult | null; busy: boolean; pending: string }>()
const copied = ref(false)
const copyError = ref('')
const rows = computed(() => props.result?.values.map((value, index) => ({ index, value })) ?? [])
const columns: DataTableColumns<{ index: number; value: string }> = [
  { title: '索引', key: 'index', width: 80, render: row => h('span', { class: 'result-index' }, String(row.index).padStart(2, '0')) },
  { title: '读取值', key: 'value', render: row => h('span', { class: 'result-value' }, props.result?.dataType.endsWith('string') ? JSON.stringify(row.value) : row.value) },
]
watch(() => props.result, () => { copied.value = false; copyError.value = '' })
async function copy() {
  if (!props.result) return
  copyError.value = ''
  try { await navigator.clipboard.writeText(JSON.stringify(props.result.values, null, 2)); copied.value = true }
  catch { copyError.value = '无法访问剪贴板，请选中数据手动复制。' }
}
</script>

<template>
  <section class="panel result-panel" aria-labelledby="result-title" :aria-busy="busy">
    <div class="panel-heading">
      <span class="panel-icon"><NIcon :component="ListOutline" :size="21" /></span>
      <div><h2 id="result-title">读取结果</h2><p>{{ result ? result.address + ' · ' + result.dataType + ' · ' + result.time + ' 快照' : '真实数据，清晰呈现。' }}</p></div>
      <NTag v-if="result" class="heading-end" size="small" round :bordered="false" type="success">{{ result.elapsedMs }} ms</NTag><span v-else class="step-label heading-end">03</span>
    </div>
    <div v-if="busy" class="result-empty" role="status"><NSpin :size="24" /><strong>{{ pending }}中</strong><p>请等待当前操作完成。</p></div>
    <div v-else-if="!result" class="result-empty"><span class="empty-icon"><NIcon :component="FileTrayOutline" :size="24" /></span><strong>等待读取第一组数据</strong><p>连接设备，填写地址，然后点击「读取数据」。<br />这里仅显示实际读取结果，不展示模拟数据。</p></div>
    <template v-else>
      <NDataTable :columns="columns" :data="rows" :row-key="row => row.index" :bordered="false" :single-line="true" size="small" :pagination="rows.length > 8 ? { pageSize: 8 } : false" />
      <div class="result-toolbar"><span class="field-hint">共 {{ result.values.length }} 项 · 多包读取不保证原子快照</span><NButton size="small" quaternary @click="copy"><template #icon><NIcon :component="copied ? CheckmarkOutline : CopyOutline" /></template>{{ copied ? '已复制' : '复制结果' }}</NButton></div>
      <p v-if="result.dataType.endsWith('string')" class="field-hint">字符串以引号显示，保留空串、换行与零字节的区别。</p>
      <p v-if="copyError" class="validation-error" role="alert">{{ copyError }}</p>
    </template>
  </section>
</template>
