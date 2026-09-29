<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NIcon, NInput } from 'naive-ui'
import { ChevronForwardOutline, HardwareChipOutline, SearchOutline } from '@vicons/ionicons5'
import { deviceGroups, filterDeviceGroups } from '../deviceCatalog'

const props = defineProps<{ selectedId: string; locked: boolean }>()
const emit = defineEmits<{ select: [id: string] }>()
const query = ref('')
const expandedGroups = ref(new Set<string>())
const visibleGroups = computed(() => filterDeviceGroups(query.value))
const resultCount = computed(() => visibleGroups.value.reduce((total, group) => total + group.devices.length, 0))
const isExpanded = (id: string) => Boolean(query.value.trim()) || expandedGroups.value.has(id)

watch(() => props.selectedId, id => {
  const group = deviceGroups.find(group => group.devices.some(device => device.id === id))
  if (group) expandedGroups.value = new Set([...expandedGroups.value, group.id])
}, { immediate: true })

function toggleGroup(id: string) {
  if (query.value.trim()) return
  const expanded = new Set(expandedGroups.value)
  if (expanded.has(id)) expanded.delete(id)
  else expanded.add(id)
  expandedGroups.value = expanded
}

function selectDevice(id: string) {
  if (!props.locked) emit('select', id)
}
</script>

<template>
  <section class="device-navigator" aria-labelledby="device-navigation-title">
    <div class="device-navigation-heading"><h2 id="device-navigation-title">设备分类</h2><span>{{ resultCount }} 项</span></div>
    <NInput v-model:value="query" size="small" clearable placeholder="搜索品牌 / 系列" :input-props="{ 'aria-label': '搜索设备品牌或系列' }">
      <template #prefix><NIcon :component="SearchOutline" :size="15" /></template>
    </NInput>
    <p class="device-navigation-hint">{{ locked ? '会话已锁定，请先断开连接再切换。' : '先选品牌，再选系列或通讯类型。' }}</p>
    <nav class="device-group-list" aria-label="PLC 品牌与系列">
      <section v-for="group in visibleGroups" :key="group.id" class="device-group">
        <button type="button" class="device-group-toggle" :aria-expanded="isExpanded(group.id)" :aria-controls="`device-group-${group.id}`" :aria-label="`${group.label}，${group.devices.length} 个分类`" :title="group.description" @click="toggleGroup(group.id)">
          <NIcon class="group-chevron" :class="{ expanded: isExpanded(group.id) }" :component="ChevronForwardOutline" :size="12" />
          <NIcon :component="HardwareChipOutline" :size="17" />
          <span>{{ group.label }}</span><small>{{ group.devices.length }}</small>
        </button>
        <ul v-show="isExpanded(group.id)" :id="`device-group-${group.id}`" class="device-children">
          <li v-for="device in group.devices" :key="device.id">
            <button type="button" class="device-leaf" :class="{ selected: device.id === selectedId }" :aria-current="device.id === selectedId ? 'true' : undefined" :disabled="locked" :title="`${group.label} / ${device.label}`" @click="selectDevice(device.id)">
              <span class="device-leaf-dot" aria-hidden="true"></span><span>{{ device.label }}</span><span v-if="device.id === selectedId" class="device-selected-mark" aria-hidden="true"></span>
            </button>
          </li>
        </ul>
      </section>
      <p v-if="!visibleGroups.length" class="device-search-empty" role="status">未找到匹配的设备分类。<br />试试「西门子」或「1200」。</p>
    </nav>
  </section>
</template>

<style scoped>
.device-navigator { display: flex; flex-direction: column; flex: 1; min-height: 100px; margin-top: 24px; }
.device-navigation-heading { display: flex; justify-content: space-between; align-items: center; padding: 0 8px; margin-bottom: 10px; }
.device-navigation-heading h2 { font-size: 11px; font-weight: 650; color: #53667f; }
.device-navigation-heading > span { color: #7d8ca0; font-size: 10px; }
.device-navigation-hint { font-size: 10px; color: #718198; line-height: 1.6; padding: 9px 4px 11px; }
.device-group-list { overflow-y: auto; overscroll-behavior: contain; scrollbar-width: thin; min-height: 0; padding-right: 3px; }
.device-group + .device-group { margin-top: 9px; }
.device-group-toggle { display: flex; align-items: center; gap: 7px; width: 100%; min-height: 36px; padding: 6px 7px; border: 0; border-radius: 7px; background: transparent; color: #394e69; text-align: left; font-size: 12px; font-weight: 600; }
.device-group-toggle:hover, .device-leaf:not(:disabled):hover { background: rgb(255 255 255 / 48%); }
.device-group-toggle > span { flex: 1; }
.device-group-toggle small { font-size: 10px; font-weight: 400; color: #7c8da3; }
.group-chevron { transition: transform .15s ease; }
.group-chevron.expanded { transform: rotate(90deg); }
.device-children { margin: 2px 0 0 20px; padding: 0 0 0 8px; list-style: none; border-left: 1px solid rgb(105 130 163 / 22%); }
.device-leaf { display: flex; align-items: center; gap: 9px; width: 100%; min-height: 34px; padding: 7px 9px; margin: 2px 0; border: 1px solid transparent; border-radius: 7px; background: transparent; color: #60718a; font-size: 11px; text-align: left; }
.device-leaf.selected { color: #0968c5; background: rgb(255 255 255 / 78%); border-color: rgb(255 255 255 / 92%); box-shadow: 0 2px 7px rgb(62 91 130 / 8%); font-weight: 650; }
.device-leaf:disabled { cursor: not-allowed; }
.device-leaf:disabled:not(.selected) { opacity: .55; }
.device-leaf-dot { width: 4px; height: 4px; border-radius: 50%; background: currentColor; opacity: .6; flex-shrink: 0; }
.device-selected-mark { width: 5px; height: 5px; border-radius: 50%; background: #0878ed; margin-left: auto; flex-shrink: 0; }
.device-search-empty { padding: 18px 8px; color: #718198; font-size: 11px; line-height: 1.8; }
@media (min-width: 761px) and (max-height: 760px) {
  .device-navigator { margin-top: 14px; }
}
@media (max-width: 760px) {
  .device-navigator { margin-top: 14px; flex: none; }
  .device-group-list { max-height: 240px; }
}
@media (forced-colors: active) {
  .device-leaf.selected { border-color: Highlight; color: Highlight; }
}
</style>
