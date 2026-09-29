<script setup lang="ts">
import { computed, ref } from 'vue'
import { NButton, NEmpty, NIcon, NInput, NPopconfirm, NSelect, NTag } from 'naive-ui'
import { SearchOutline, TimeOutline, TrashOutline } from '@vicons/ionicons5'
import type { LogEntry } from '../types'

const props = defineProps<{ entries: LogEntry[] }>()
const emit = defineEmits<{ clear: [] }>()
const search = ref('')
const level = ref('all')
const levels = [{ label: '全部状态', value: 'all' }, { label: '成功', value: 'ok' }, { label: '错误', value: 'error' }, { label: '信息', value: 'info' }]
const filtered = computed(() => props.entries.filter(entry => (level.value === 'all' || entry.level === level.value) && entry.message.toLowerCase().includes(search.value.trim().toLowerCase())))
</script>

<template>
  <section class="panel logs-panel" aria-labelledby="logs-title">
    <div class="panel-heading"><span class="panel-icon"><NIcon :component="TimeOutline" :size="21" /></span><div><h2 id="logs-title">通讯日志</h2><p>当前窗口最近 100 条记录，关闭后不保留。</p></div><NTag class="heading-end" size="small" :bordered="false" round>{{ entries.length }} 条记录</NTag></div>
    <div class="log-toolbar">
      <NInput v-model:value="search" clearable placeholder="搜索地址、操作或错误信息" :input-props="{ 'aria-label': '搜索日志' }"><template #prefix><NIcon :component="SearchOutline" /></template></NInput>
      <NSelect v-model:value="level" :options="levels" aria-label="日志状态筛选" />
      <NPopconfirm @positive-click="emit('clear')"><template #trigger><NButton quaternary :disabled="!entries.length"><template #icon><NIcon :component="TrashOutline" /></template>清空</NButton></template>清空当前窗口的所有通讯记录？此操作不可撤销。</NPopconfirm>
    </div>
    <div v-if="!filtered.length" class="result-empty"><NEmpty :description="entries.length ? '没有匹配的记录，试试其他关键词。' : '暂无记录，设备操作将在这里留下足迹。'" /></div>
    <div v-else class="log-scroll" role="log" aria-label="通讯记录" aria-live="polite" aria-relevant="additions" tabindex="0">
      <div v-for="entry in filtered" :key="entry.id" class="log-row"><time>{{ entry.time }}</time><NTag :type="entry.level === 'ok' ? 'success' : entry.level === 'error' ? 'error' : 'default'" size="small" round :bordered="false">{{ entry.level === 'ok' ? '成功' : entry.level === 'error' ? '错误' : '信息' }}</NTag><p>{{ entry.message }}</p></div>
    </div>
  </section>
</template>
