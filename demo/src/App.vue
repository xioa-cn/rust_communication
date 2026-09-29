<script setup lang="ts">
import { computed, defineAsyncComponent, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { NAlert, NBadge, NButton, NConfigProvider, NIcon, NModal, zhCN, dateZhCN } from 'naive-ui'
import { ArrowForwardOutline, ChevronForwardOutline, GridOutline, HardwareChipOutline, ShieldCheckmarkOutline, TimeOutline } from '@vicons/ionicons5'
import ConnectionPanel from './components/ConnectionPanel.vue'
import DeviceNavigator from './components/DeviceNavigator.vue'
import DataConsole from './components/DataConsole.vue'
import AdvancedTools from './components/AdvancedTools.vue'
import { useS7 } from './composables/useS7'
import { workbenchTheme } from './theme'
import { isSerialProtocol, protocolLabels } from './types'
import { createDeviceConnection, deviceGroups, getConnectedDevice, getDeviceProfile } from './deviceCatalog'
import type { ConnectRequest, PlcProtocol, ReadRequest, WriteRequest } from './types'
import type { RustOperation } from './rustExamples'
import './workbench.css'
import './single-screen.css'

const { isDesktop, connection, busy, pending, error, result, logs, connect, disconnect, read, write, clearLogs } = useS7()
const ioRunning = ref(false), toolsRunning = ref(false)
const taskActive = computed(() => ioRunning.value || toolsRunning.value)
const dataConsole = ref<{ stop: () => void } | null>(null), advancedTools = ref<{ stop: () => void; showCode: () => void } | null>(null)
const configuration = ref<ConnectRequest | null>(null)
const exampleConnection = ref<ConnectRequest | null>(null)
const exampleOperation = ref<RustOperation>({ kind: 'connect' })
const connectionOpen = ref(false)
const endpoint = computed(() => configuration.value ? isSerialProtocol(configuration.value.protocol) ? configuration.value.serial.path : `${configuration.value.host}:${configuration.value.port ?? '—'}` : '配置连接参数')
function stopTasks() { dataConsole.value?.stop(); advancedTools.value?.stop() }
async function connectDevice(request: ConnectRequest) {
  exampleConnection.value = JSON.parse(JSON.stringify(request))
  showExample({ kind: 'connect' })
  await connect(request)
  if (connection.value.connected) connectionOpen.value = false
}
function showExample(operation: RustOperation, reveal = true) {
  exampleOperation.value = JSON.parse(JSON.stringify(operation))
  if (reveal) advancedTools.value?.showCode()
}
async function readWithExample(request: ReadRequest) {
  showExample({ kind: 'read', request }, false)
  return read(request)
}
async function writeWithExample(request: WriteRequest) {
  showExample({ kind: 'write', request }, false)
  return write(request)
}
const ActivityLog = defineAsyncComponent(() => import('./components/ActivityLog.vue'))
const activePage = ref<'workspace' | 'logs'>('workspace')
const selectedDeviceId = ref('s7-1200')
const codeConfiguration = computed(() => exampleConnection.value ?? configuration.value ?? createDeviceConnection(selectedDeviceId.value))
const selectedDevice = computed(() => getDeviceProfile(selectedDeviceId.value))
const selectedGroup = computed(() => deviceGroups.find(group => group.devices.some(device => device.id === selectedDeviceId.value))!)
const pageTitle = computed(() => activePage.value === 'workspace' ? `${selectedDevice.value.label} 工作台` : '活动记录')
const selectedProtocol = ref<PlcProtocol>('s7')
const activeProtocol = computed(() => connection.value.connected ? connection.value.protocol ?? selectedProtocol.value : selectedProtocol.value)
const activeInovanceSeries = computed(() => connection.value.connected ? connection.value.inovanceSeries ?? codeConfiguration.value.inovance.series : configuration.value?.inovance.series ?? selectedDevice.value.inovanceSeries?.[0] ?? 'AM')
const protocolLabel = computed(() => protocolLabels[activeProtocol.value])
const statusText = computed(() => busy.value ? pending.value + '中…' : taskActive.value ? '任务执行中' : connection.value.connected ? '已连接' : '未连接')

watch(connection, status => {
  const device = getConnectedDevice(status)
  if (device) selectedDeviceId.value = device.id
  if (status.connected && status.protocol) selectedProtocol.value = status.protocol
})

function selectDevice(id: string) {
  if (busy.value || taskActive.value || connection.value.connected) return
  const profile = getDeviceProfile(id)
  if (id !== selectedDeviceId.value) {
    selectedDeviceId.value = id
    exampleConnection.value = null
    configuration.value = null
    exampleOperation.value = { kind: 'connect' }
    selectedProtocol.value = profile.protocols[0]
    result.value = null
    error.value = ''
  }
  activePage.value = 'workspace'
}

async function windowAction(action: 'close' | 'minimize' | 'toggleMaximize') {
  if (!isDesktop || (action === 'close' && (busy.value || taskActive.value))) return
  try { await getCurrentWindow()[action]() }
  catch (cause) { error.value = '窗口操作未完成：' + String(cause) }
}
</script>

<template>
  <NConfigProvider :theme-overrides="workbenchTheme" :locale="zhCN" :date-locale="dateZhCN">
    <div class="app-shell single-screen">
      <a class="skip-link" href="#main-content">跳转到工作区</a>
      <div class="ambient-orb ambient-orb-one" aria-hidden="true"></div>
      <div class="ambient-orb ambient-orb-two" aria-hidden="true"></div>
      <header class="global-header" data-tauri-drag-region>
        <div class="header-inner" data-tauri-drag-region>
          <div class="header-leading">
            <span class="window-title" data-tauri-drag-region>Workbench</span>
          </div>
          <div class="toolbar-context" data-tauri-drag-region><span data-tauri-drag-region>本地工作区</span><NIcon :component="ChevronForwardOutline" :size="11" /><strong data-tauri-drag-region>{{ pageTitle }}</strong></div>
          <div class="window-drag-space" data-tauri-drag-region @dblclick="windowAction('toggleMaximize')"></div>
          <div class="header-actions">
            <div class="header-status" role="status"><span class="status-dot" :class="{ online: connection.connected }"></span>{{ statusText }}</div>
            <div class="window-controls" role="group" aria-label="窗口控制">
              <button class="window-control minimize" :disabled="!isDesktop" aria-label="最小化窗口" :title="isDesktop ? '最小化窗口' : '仅桌面应用可用'" @click="windowAction('minimize')"><svg class="control-icon" viewBox="0 0 12 12" aria-hidden="true" focusable="false"><path d="M1.75 6h8.5" /></svg></button>
              <button class="window-control maximize" :disabled="!isDesktop" aria-label="最大化或还原窗口" :title="isDesktop ? '最大化或还原窗口' : '仅桌面应用可用'" @click="windowAction('toggleMaximize')"><svg class="control-icon" viewBox="0 0 12 12" aria-hidden="true" focusable="false"><rect x="2.25" y="2.25" width="7.5" height="7.5" rx=".5" /></svg></button>
              <button class="window-control close" :disabled="!isDesktop || busy || taskActive" aria-label="关闭窗口" :title="taskActive ? '请先停止当前任务' : isDesktop ? '关闭窗口' : '仅桌面应用可用'" @click="windowAction('close')"><svg class="control-icon" viewBox="0 0 12 12" aria-hidden="true" focusable="false"><path d="m2.5 2.5 7 7m0-7-7 7" /></svg></button>
            </div>
          </div>
        </div>
      </header>
      <div class="window-body">
        <aside class="app-sidebar" aria-label="工作区侧边栏">
          <a class="sidebar-brand" href="#" aria-label="Rust Workbench 首页" @click.prevent="activePage = 'workspace'"><span class="app-mark"><img src="/rust-logo.svg" alt="" width="25" height="25" /></span><span>Rust Workbench<small>PLC 通讯工具</small></span></a>
          <p class="sidebar-section-label">工作空间</p>
          <nav class="main-nav" aria-label="主导航">
            <button :class="{ active: activePage === 'workspace' }" :aria-current="activePage === 'workspace' ? 'page' : undefined" title="设备工作台" @click="activePage = 'workspace'"><NIcon :component="GridOutline" :size="18" /><span class="nav-label">工作台</span></button>
            <button :class="{ active: activePage === 'logs' }" :aria-current="activePage === 'logs' ? 'page' : undefined" title="活动记录" @click="activePage = 'logs'"><NIcon :component="TimeOutline" :size="18" /><span class="nav-label">活动记录</span><NBadge :value="logs.length" :max="99" color="#dfe7f1" /></button>
          </nav>
          <DeviceNavigator :selected-id="selectedDeviceId" :locked="busy || taskActive || connection.connected" @select="selectDevice" />
          <div class="sidebar-session"><p class="sidebar-section-label">当前选择</p><div class="sidebar-device"><NIcon :component="HardwareChipOutline" :size="19" /><div><strong>{{ selectedGroup.label }} · {{ selectedDevice.label }}</strong><span>{{ connection.connected ? protocolLabel + ' 会话' : '尚未连接 · 等待手动连接' }}</span></div><span class="status-dot" :class="{ online: connection.connected }"></span></div></div>
          <div class="sidebar-bottom"><NIcon :component="ShieldCheckmarkOutline" :size="17" /><span>本地运行<small>手动操作，始终由你掌控。</small></span></div>
        </aside>
        <div class="workspace-frame">
          <main id="main-content" class="workspace-body studio-workspace" tabindex="-1">
            <section class="intro-row" aria-labelledby="page-title">
              <div class="intro-copy"><p class="eyebrow">{{ activePage === 'workspace' ? 'DEVICE WORKSPACE' : 'SESSION ACTIVITY' }}</p><h1 id="page-title">{{ pageTitle }}</h1><p class="intro-description">{{ activePage === 'workspace' ? `${selectedGroup.label} / ${selectedDevice.label} · 配置连接并手动读写设备。` : '本次会话的每一步，都清晰可查。' }}</p></div>
              <div class="session-summary"><div class="session-specs"><div><span>通讯协议</span><strong>{{ protocolLabel }}</strong></div><div><span>协商 PDU</span><strong>{{ connection.pduLength ?? '—' }}<small v-if="connection.pduLength"> B</small></strong></div></div></div>
            </section>
            <div v-if="!isDesktop" class="preview-note" role="status"><NIcon :component="ShieldCheckmarkOutline" :size="15" /><span>浏览器预览 · 参数可编辑，设备通讯请在桌面应用中执行。</span><code>npm run tauri:dev</code></div>
            <NAlert v-if="error" class="error-banner" type="error" title="操作未完成" closable @close="error = ''">{{ error }}</NAlert>
            <div v-if="taskActive && activePage === 'logs'" class="task-banner global-task-banner" role="status"><span class="status-dot online"></span>有任务正在执行，设备切换与连接配置已锁定。<NButton size="small" type="warning" @click="stopTasks">停止所有任务</NButton></div>
            <div v-show="activePage === 'workspace'" class="workspace-stack">
              <div class="connection-disclosure">
                <div v-if="taskActive" class="task-banner global-task-banner" role="status"><span class="status-dot online"></span>有任务正在执行，设备切换与连接配置已锁定。<NButton size="small" type="warning" @click="stopTasks">停止所有任务</NButton></div>
                <button v-else type="button" class="connection-trigger" aria-haspopup="dialog" :aria-expanded="connectionOpen" @click="connectionOpen = true"><span class="studio-icon"><NIcon :component="HardwareChipOutline" :size="18" /></span><span class="connection-trigger-copy"><strong>连接配置</strong><span>{{ selectedGroup.label }} / {{ selectedDevice.label }} · {{ endpoint }}</span></span><span class="connection-state" :class="{ online: connection.connected }">{{ connection.connected ? '已连接' : '未连接' }}</span><span class="connection-expand-label">配置与连接</span></button>
                <NModal v-model:show="connectionOpen" class="connection-dialog" preset="card" title="设备连接配置" display-directive="show" :style="{ width: 'min(660px, calc(100vw - 40px))' }"><ConnectionPanel :device-id="selectedDeviceId" :connection="connection" :busy="busy || taskActive" :pending="pending" :desktop="isDesktop" @connect="connectDevice" @disconnect="disconnect" @protocol-change="selectedProtocol = $event" @configuration-change="configuration = $event" /></NModal>
              </div>
              <DataConsole :key="selectedDeviceId + '-data'" ref="dataConsole" :protocol="activeProtocol" :inovance-series="activeInovanceSeries" :connected="connection.connected" :desktop="isDesktop" :busy="busy" :locked="taskActive" :read="read" :write="write" @example="showExample" @running="ioRunning = $event" />
              <AdvancedTools :key="selectedDeviceId + '-tools'" ref="advancedTools" :protocol="activeProtocol" :connected="connection.connected" :desktop="isDesktop" :busy="busy" :locked="taskActive" :connection-config="codeConfiguration" :example-operation="exampleOperation" :read="readWithExample" :write="writeWithExample" @running="toolsRunning = $event" />
            </div>
            <ActivityLog v-if="activePage === 'logs'" :entries="logs" @clear="clearLogs" />
            <div v-if="activePage === 'workspace'" class="workspace-caption"><span><span class="status-dot" :class="{ online: connection.connected }"></span>{{ connection.connected ? '会话已建立 · 状态非实时心跳' : '就绪 · 等待手动连接' }}</span><NButton text type="primary" @click="activePage = 'logs'">查看活动记录<template #icon><NIcon :component="ArrowForwardOutline" /></template></NButton></div>
          </main>
          <footer class="app-footer"><span>所有设备通讯仅在本机执行</span><span>{{ protocolLabel }}<span class="footer-separator">·</span>{{ isDesktop ? '桌面应用' : '浏览器预览' }}</span></footer>
        </div>
      </div>
    </div>
  </NConfigProvider>
</template>
