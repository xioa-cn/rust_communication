import assert from 'node:assert/strict'
import { writeFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

const debugUrl = process.env.PLC_TEST_DEBUG_URL || 'http://127.0.0.1:9342'
const appUrl = process.env.PLC_TEST_APP_URL || 'http://127.0.0.1:1462'
const toolIds = ['batch', 'raw', 'points', 'addresses', 'code']
const output = join(tmpdir(), 'plc-studio-browser-check')
await mkdir(output, { recursive: true })
const targets = await (await fetch(`${debugUrl}/json/list`)).json()
const target = targets.find(target => target.type === 'page')
const socket = new WebSocket(target.webSocketDebuggerUrl)
await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }))
let commandId = 0
const commands = new Map(), errors = []
socket.addEventListener('message', event => {
  const message = JSON.parse(event.data)
  if (message.id) {
    const handler = commands.get(message.id)
    if (!handler) return
    commands.delete(message.id)
    message.error ? handler.reject(new Error(message.error.message)) : handler.resolve(message.result)
  } else if (message.method === 'Runtime.exceptionThrown') errors.push(message.params.exceptionDetails)
})
function cdp(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++commandId, timer = setTimeout(() => reject(new Error(`CDP timeout: ${method}`)), 15000)
    commands.set(id, { resolve: value => { clearTimeout(timer); resolve(value) }, reject: error => { clearTimeout(timer); reject(error) } })
    socket.send(JSON.stringify({ id, method, params }))
  })
}
async function evaluate(expression) {
  const response = await cdp('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
  if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails))
  return response.result.value
}
async function waitFor(expression) {
  for (let attempt = 0; attempt < 400; attempt++) { if (await evaluate(expression)) return; await delay(50) }
  throw new Error(`Condition failed: ${expression}; errors=${JSON.stringify(errors)}; page=${await evaluate('document.body.innerText.slice(0, 2000)')}`)
}
async function click(selector) {
  await evaluate(`(() => { const element = document.querySelector(${JSON.stringify(selector)}); if (!element) throw new Error('Missing: ' + ${JSON.stringify(selector)}); element.click(); })()`)
  await delay(70)
}
async function clickText(scope, text) {
  await evaluate(`(() => { const button = [...document.querySelectorAll(${JSON.stringify(scope + ' button')})].find(button => button.textContent.trim() === ${JSON.stringify(text)}); if (!button) throw new Error('Missing button: ' + ${JSON.stringify(text)}); button.click(); })()`)
  await delay(80)
}
async function input(selector, value) {
  await evaluate(`(() => { const input = document.querySelector(${JSON.stringify(selector)}); if (!input) throw new Error('Missing input'); input.focus(); input.value = ${JSON.stringify(String(value))}; input.dispatchEvent(new Event('input', { bubbles: true })); input.dispatchEvent(new Event('change', { bubbles: true })); input.blur(); })()`)
  await delay(70)
}
async function field(scope, label, value) {
  await evaluate(`(() => { const label = [...document.querySelectorAll(${JSON.stringify(scope + ' label')})].find(label => label.textContent.trim().startsWith(${JSON.stringify(label)})); const input = label?.querySelector('input,select,textarea'); if (!input) throw new Error('Missing field: ' + ${JSON.stringify(label)}); input.focus(); input.value = ${JSON.stringify(String(value))}; input.dispatchEvent(new Event('input', { bubbles: true })); input.dispatchEvent(new Event('change', { bubbles: true })); input.blur(); })()`)
  await delay(70)
}
async function tab(id) { await click(`#tool-tab-${id}`) }
async function assertSingleScreen(label) {
  const overflow = await evaluate(`(() => {
    const issues = [];
    const workspace = document.querySelector('.studio-workspace');
    if (workspace.scrollHeight > workspace.clientHeight + 2) issues.push('workspace vertical overflow');
    if (document.documentElement.scrollHeight > innerHeight + 2) issues.push('document vertical overflow');
    for (const panel of document.querySelectorAll('.read-card, .write-card, .advanced-tools')) {
      const bounds = panel.getBoundingClientRect();
      if (bounds.bottom > innerHeight + 2) issues.push(panel.className + ' below viewport');
      for (const element of panel.querySelectorAll('button, input, select, textarea')) {
        if (!element.getClientRects().length || element.closest('.table-scroll, .code-output')) continue;
        const rect = element.getBoundingClientRect();
        if (rect.width < 1 || rect.height < 1) continue;
        const fieldset = element.closest('.studio-fieldset');
        if (fieldset && rect.bottom > fieldset.getBoundingClientRect().bottom + 2) issues.push((element.getAttribute('aria-label') || element.type) + ' overlaps content after fieldset by ' + (rect.bottom - fieldset.getBoundingClientRect().bottom));
        if (rect.top < bounds.top - 2 || rect.bottom > bounds.bottom + 2 || rect.left < bounds.left - 2 || rect.right > bounds.right + 2) {
          issues.push((element.getAttribute('aria-label') || element.textContent.trim() || element.type) + ' outside ' + panel.className);
        }
      }
    }
    return issues;
  })()`)
  if (overflow.length) {
    const capture = await cdp('Page.captureScreenshot', { format: 'png' })
    await writeFile(join(output, 'layout-failure.png'), Buffer.from(capture.data, 'base64'))
  }
  assert.deepEqual(overflow, [], label)
}
async function expandConnectionSettings() {
  const expanded = `Boolean(document.querySelector('input[aria-label="连接超时"]')?.getClientRects().length)`
  if (!await evaluate(expanded)) {
    await click('.advanced-settings .n-collapse-item__header-main')
  }
  await waitFor(expanded)
}
async function assertConnectionDialog(label) {
  await delay(300)
  const bounds = await evaluate(`(() => {
    const dialog = document.querySelector('.connection-dialog');
    const rect = dialog.getBoundingClientRect();
    const container = document.querySelector('.n-modal-scroll-content');
    return { fits: rect.top >= 0 && rect.bottom <= innerHeight && rect.left >= 0 && rect.right <= innerWidth, scrolls: container ? container.scrollHeight > container.clientHeight + 2 : false };
  })()`)
  assert.deepEqual(bounds, { fits: true, scrolls: false }, label)
  const layoutIssues = await evaluate(`(() => {
    const issues = [];
    const dialog = document.querySelector('.connection-dialog');
    const context = dialog.querySelector('.connection-context').getBoundingClientRect();
    const footer = dialog.querySelector('.connection-footer').getBoundingClientRect();
    const state = dialog.querySelector('.connection-session-state').getBoundingClientRect();
    const actions = dialog.querySelectorAll('.connection-footer > button');
    if (actions.length !== 1) issues.push('expected one connection action');
    const action = actions[0].getBoundingClientRect();
    if (Math.abs(action.right - footer.right) > 2 || action.left < state.right) issues.push('footer alignment');
    for (const grid of dialog.querySelectorAll('.connection-grid')) {
      if (!grid.getClientRects().length) continue;
      const rect = grid.getBoundingClientRect();
      if (Math.abs(rect.left - context.left) > 2 || Math.abs(rect.right - context.right) > 2) issues.push('field group alignment');
    }
    for (const control of dialog.querySelectorAll('input, .n-base-selection')) {
      if (!control.getClientRects().length) continue;
      const rect = control.getBoundingClientRect();
      if (rect.left < context.left - 2 || rect.right > context.right + 2 || rect.bottom > footer.top) issues.push('clipped control');
    }
    for (const control of dialog.querySelectorAll('.connection-grid .n-input-number')) {
      if (!control.getClientRects().length) continue;
      if (Math.abs(control.getBoundingClientRect().width - control.closest('.n-form-item').getBoundingClientRect().width) > 2) issues.push('numeric field width');
    }
    return issues;
  })()`)
  assert.deepEqual(layoutIssues, [], label + ': aligned fields and footer')
}
async function screenshot(name, width, height, position = 'top') {
  await cdp('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await evaluate(position === 'top' ? 'document.querySelector(".workspace-body").scrollTop = 0; window.scrollTo(0, 0)' : 'document.querySelector(".advanced-tools").scrollIntoView()')
  await delay(200)
  assert.equal(await evaluate('document.documentElement.scrollWidth > innerWidth'), false, `${width}px horizontal overflow`)
  const screenshot = await cdp('Page.captureScreenshot', { format: 'png' })
  await writeFile(join(output, `${name}.png`), Buffer.from(screenshot.data, 'base64'))
}
let injection
try {
  await cdp('Runtime.enable')
  await cdp('Page.enable')
  await cdp('Page.navigate', { url: appUrl })
  await waitFor('Boolean(document.querySelector(".read-card"))')
  assert.deepEqual(await evaluate('[...document.querySelectorAll(".tool-tabs button")].map(button => button.textContent.trim())'), ['批量读取', '报文读取', '点位变量', '地址示例', '代码示例'])
  assert.equal(await evaluate('document.querySelector(".advanced-tools .studio-badge").textContent'), '5 个工具')
  assert.equal(await evaluate('["remote", "threads", "simulation", "export", "special"].some(id => document.getElementById("tool-tab-" + id) || document.getElementById("tool-panel-" + id))'), false)
  await tab('code')
  await evaluate('document.querySelector("#tool-tab-code").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }))')
  await waitFor('document.querySelector("#tool-tab-batch").getAttribute("aria-selected") === "true"')
  assert.equal(await evaluate('document.querySelectorAll(".device-leaf").length'), 20)
  await screenshot('desktop', 1440, 1000)
  await screenshot('narrow', 820, 640)
  for (const [width, height] of [[1440, 1000], [1280, 900], [1366, 768], [820, 640]]) {
    await cdp('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    for (const id of toolIds) {
      await tab(id)
      await assertSingleScreen(`${width}x${height}: ${id}`)
    }
  }
  await click('.connection-trigger')
  await expandConnectionSettings()
  await waitFor('Boolean(document.querySelector(\'.connection-dialog input[placeholder="0100"]\')?.getClientRects().length)')
  await assertConnectionDialog('820x640: expanded S7 settings')
  await screenshot('connection-narrow', 820, 640)
  await input('input[aria-label="IP 地址"]', '192.168.0.123')
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', '三菱3')
  await clickText('.device-group-list', 'MC')
  await click('.connection-trigger')
  await expandConnectionSettings()
  await assertConnectionDialog('820x640: expanded Mitsubishi settings')
  await screenshot('connection-mitsubishi-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', '通用 Modbus4')
  await clickText('.device-group-list', 'RTU · 串口')
  await click('.connection-trigger')
  await assertConnectionDialog('820x640: Modbus serial settings')
  await screenshot('connection-serial-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', '欧姆龙2')
  await clickText('.device-group-list', 'FINS · TCP')
  await click('.connection-trigger')
  await expandConnectionSettings()
  await assertConnectionDialog('820x640: expanded FINS TCP route settings')
  assert.equal(await evaluate(`document.querySelector('input[aria-label="FINS 端口，默认 9600"]').value`), '9600')
  assert.ok(await evaluate(`document.querySelector('[aria-label="FINS 字节序"]').textContent.includes('CDAB')`))
  await input('input[aria-label="FINS 源节点 SA1"]', 20)
  await input('input[aria-label="FINS 目标节点 DA1"]', 10)
  await input('input[aria-label="FINS 目标网络 · DNA"]', 2)
  await screenshot('connection-omron-tcp-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', 'FINS · UDP')
  await click('.connection-trigger')
  await expandConnectionSettings()
  assert.equal(await evaluate(`document.querySelector('input[aria-label="FINS 源节点 SA1"]').value`), '0')
  assert.equal(await evaluate(`document.querySelector('input[aria-label="FINS 目标网络 · DNA"]').value`), '0')
  await assertConnectionDialog('820x640: expanded FINS UDP route settings')
  await screenshot('connection-omron-udp-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', 'FINS · TCP')
  await click('.connection-trigger')
  assert.equal(await evaluate(`document.querySelector('input[aria-label="FINS 源节点 SA1"]').value`), '20')
  assert.equal(await evaluate(`document.querySelector('input[aria-label="FINS 目标网络 · DNA"]').value`), '2')
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', 'S7-1200')
  await click('.connection-trigger')
  assert.equal(await evaluate('document.querySelector(\'input[aria-label="IP 地址"]\').value'), '192.168.0.123')
  await click('.connection-dialog .n-card-header__close')
  await screenshot('mobile', 390, 844)
  await cdp('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false })
  for (const id of toolIds) {
    await tab(id)
    assert.equal(await evaluate(`Boolean(document.querySelector('#tool-panel-${id} h3'))`), true)
  }
  await tab('raw')
  await input('textarea[aria-label="HEX 内容"]', '00 01 00 02')
  await field('#tool-panel-raw', '解释类型', 'u16')
  assert.equal(await evaluate('document.querySelector("#tool-panel-raw .code-output").textContent'), '1, 2')
  await field('#tool-panel-raw', '查找字符串', '2')
  await clickText('#tool-panel-raw', '查找')
  await waitFor('document.querySelector("#tool-panel-raw").textContent.includes("找到 1 处")')
  console.log('PASS: five retained tools, HEX parsing, worker search, desktop/narrow/mobile layout')

  injection = await cdp('Page.addScriptToEvaluateOnNewDocument', { source: `
    window.isTauri = true;
    window.__calls = { read: [], write: [], connect: [] };
    window.__status = { connected: false, protocol: null, cpu: null, pduLength: null, inovanceSeries: null };
    window.__inflight = 0; window.__maxInflight = 0; window.__failNext = false;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      if (command === 'status') return { ...window.__status };
      if (command === 'connect') { window.__calls.connect.push(args.request); window.__status = { connected: true, protocol: args.request.protocol, cpu: args.request.protocol === 's7' ? args.request.cpu : null, pduLength: args.request.protocol === 's7' ? 480 : null, inovanceSeries: args.request.protocol === 'inovance_modbus_tcp' ? args.request.inovance.series : null }; return { ...window.__status }; }
      if (command === 'disconnect') { window.__status = { connected: false, protocol: null, cpu: null, pduLength: null, inovanceSeries: null }; return { ...window.__status }; }
      if (!window.__status.connected) throw new Error('Mock disconnected');
      if (command === 'read') {
        window.__calls.read.push(args.request); window.__inflight++; window.__maxInflight = Math.max(window.__maxInflight, window.__inflight);
        try { await new Promise(resolve => setTimeout(resolve, 25)); if (window.__failNext) { window.__failNext = false; throw new Error('Mock read failure'); }
          const values = args.request.dataType === 'bool' ? ['true'] : args.request.dataType === 'u64' ? ['18446744073709551615'] : ['raw_string', 's7_string'].includes(args.request.dataType) ? ['ABC'] : Array.from({ length: args.request.length || 1 }, (_, index) => String(args.request.dataType === 'u8' ? 65 + index % 26 : 100 + index));
          return { values, elapsedMs: 5 };
        } finally { window.__inflight--; }
      }
      if (command === 'write') { if (!args.request.confirmed) throw new Error('Missing write confirmation'); window.__calls.write.push(args.request); await new Promise(resolve => setTimeout(resolve, 15)); return { count: args.request.values.length, unit: '项', elapsedMs: 4 }; }
      throw new Error('Unexpected mock command: ' + command);
    }};
  ` })
  await cdp('Page.reload')
  await waitFor('Boolean(window.__calls) && Boolean(document.querySelector(".connection-trigger")) && !document.querySelector(".preview-note")')
  await click('.connection-trigger')
  await waitFor('Boolean(document.querySelector(".connect-action")) && !document.querySelector(".connect-action").disabled')
  await cdp('Emulation.setDeviceMetricsOverride', { width: 820, height: 640, deviceScaleFactor: 1, mobile: false })
  await expandConnectionSettings()
  await input('input[aria-label="本地 TSAP"]', '0100')
  await click('.connect-action')
  await waitFor('Boolean(document.querySelector(".connection-panel .validation-error"))')
  assert.equal(await evaluate('window.__calls.connect.length'), 0)
  await assertConnectionDialog('820x640: expanded S7 with validation error')
  await input('input[aria-label="本地 TSAP"]', '')
  await click('.advanced-settings .n-collapse-item__header-main')
  await cdp('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false })
  await click('.connect-action')
  await waitFor('window.__status.connected')
  await waitFor('document.querySelector(".connection-trigger").getAttribute("aria-expanded") === "false"')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="创建 PLC Rust 示例"]\').textContent.includes("S7Net::new")'))
  assert.ok(await evaluate('document.querySelector(\'[aria-label="当前操作 Rust 示例"]\').textContent.includes("plc.connect()")'))
  await clickText('.read-card', '读取 u16')
  await waitFor('document.querySelector(".read-output").textContent.includes("100")')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="当前操作 Rust 示例"]\').textContent.includes(\'read::<u16>("DB1.0", 1)\')'))
  await clickText('.read-card .type-chips', 'ULong')
  await clickText('.read-card', '读取 u64')
  await clickText('.segmented-control', 'Hex')
  assert.ok(await evaluate('document.querySelector(".read-output").textContent.includes("0xFFFFFFFFFFFFFFFF")'))
  await clickText('.read-card .type-chips', 'String')
  await field('.read-card', '数量 / 字节', 3)
  await clickText('.read-card', '读取 raw_string')
  assert.ok(await evaluate('document.querySelector(".read-output").textContent.includes("ABC")'))
  assert.ok(await evaluate('document.querySelector(\'[aria-label="当前操作 Rust 示例"]\').textContent.includes("String::from_utf8")'))
  await clickText('.read-card .type-chips', 'UShort')
  await input('input[aria-label="定时读取间隔"]', 200)
  const readsBeforeTimer = await evaluate('window.__calls.read.length')
  await clickText('.read-card', '定时读取')
  await waitFor(`window.__calls.read.length >= ${readsBeforeTimer + 2}`)
  await clickText('.global-task-banner', '停止所有任务')
  await waitFor('!document.querySelector(".global-task-banner")')
  const stoppedReadCount = await evaluate('window.__calls.read.length')
  await delay(350)
  assert.equal(await evaluate('window.__calls.read.length'), stoppedReadCount)
  assert.equal(await evaluate('window.__maxInflight'), 1)
  await input('textarea[aria-label="写入值"]', '[1:3]')
  await clickText('.write-card', '写入数据')
  assert.equal(await evaluate('window.__calls.write.length'), 0)
  await click('.write-confirmation input')
  await clickText('.write-card', '写入数据')
  await waitFor('window.__calls.write.length === 1')
  assert.deepEqual(await evaluate('window.__calls.write[0].values'), ['1', '2', '3'])
  assert.equal(await evaluate('document.querySelector(".write-confirmation input").checked'), false)
  await click('.write-confirmation input')
  await input('input[aria-label="写入地址"]', 'DB1.2')
  assert.equal(await evaluate('document.querySelector(".write-confirmation input").checked'), false)
  await input('input[aria-label="定时写入次数"]', 2)
  await input('input[aria-label="定时写入间隔"]', 200)
  await click('.write-confirmation input')
  await clickText('.write-card', '定时写入')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.equal(await evaluate('window.__calls.write.length'), 3)
  await delay(250)
  assert.equal(await evaluate('window.__calls.write.length'), 3)
  console.log('PASS: typed and encoded reads, 64-bit display, non-overlapping timer stop, write confirmation and bounded scheduled writes')

  await tab('batch')
  await input('input[aria-label="批量读取地址"]', 'DB20.8')
  await input('input[aria-label="批量读取长度"]', 4)
  await clickText('#tool-panel-batch', '批量读取')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.deepEqual(await evaluate('window.__calls.read.at(-1)'), { address: 'DB20.8', dataType: 'u8', length: 4 })
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), '41 42 43 44')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="批量读取 Rust 代码"]\').textContent.includes(\'read::<u8>("DB20.8", 4)\')'))
  await click('input[aria-label="批量解析"]')
  await input('select[aria-label="批量解析类型"]', 'ascii')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), 'ABCD')
  await click('input[aria-label="批量字节交换"]')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), 'BADC')
  await click('input[aria-label="批量字节交换"]')
  await input('input[aria-label="批量查找字符串"]', 'A.C')
  await click('input[aria-label="批量正则表达式"]')
  await clickText('#tool-panel-batch', '查找')
  await waitFor('document.querySelector(".batch-feedback").textContent.includes("找到 1 处")')
  assert.ok(await evaluate('document.querySelector(".batch-selection").textContent.includes("Index: 0")'))
  await click('input[aria-label="批量解析"]')
  await input('input[aria-label="批量每行数量"]', 2)
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), '41 42\n43 44')
  await input('textarea[aria-label="批量读取结果"]', 'FF 00 01 02')
  await clickText('#tool-panel-batch', '批量回写')
  assert.equal(await evaluate('window.__calls.write.length'), 3)
  await click('input[aria-label="确认批量回写"]')
  await input('textarea[aria-label="批量读取结果"]', 'FE 00 01 02')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="确认批量回写"]\').checked'), false)
  await click('input[aria-label="确认批量回写"]')
  await clickText('#tool-panel-batch', '批量回写')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.equal(await evaluate('window.__calls.write.length'), 4)
  assert.deepEqual(await evaluate('window.__calls.write.at(-1).values'), ['254', '0', '1', '2'])
  assert.equal(await evaluate('window.__calls.write.at(-1).address'), 'DB20.8')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="确认批量回写"]\').checked'), false)
  await screenshot('batch-desktop', 1440, 1000)
  await tab('code')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="当前操作 Rust 示例"]\').textContent.includes("let confirmed = false;")'))
  await screenshot('rust-desktop', 1440, 1000)
  await tab('batch')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), 'FE 00 01 02')
  await tab('points')
  await clickText('#tool-panel-points', '＋ 添加点位')
  await clickText('#tool-panel-points', '读取点表')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.ok(await evaluate('document.querySelector(".point-table tbody code").textContent.includes("100")'))
  await screenshot('points', 1440, 1000, 'tools')
  await cdp('Emulation.setDeviceMetricsOverride', { width: 820, height: 640, deviceScaleFactor: 1, mobile: false })
  await input('select[aria-label="读取类型"]', 'raw_string')
  await input('select[aria-label="写入类型"]', 'raw_string')
  await assertSingleScreen('820x640: string encoding controls')
  for (const id of toolIds) {
    await tab(id)
    await assertSingleScreen(`820x640 connected with string controls: ${id}`)
  }
  await screenshot('strings-narrow', 820, 640)
  await clickText('.read-card', '定时读取')
  await waitFor('Boolean(document.querySelector(".global-task-banner"))')
  await assertSingleScreen('820x640: string timer and stop controls')
  await tab('batch')
  await delay(300)
  assert.ok(await evaluate('Boolean(document.querySelector("#tool-panel-batch"))'))
  await assertSingleScreen('820x640: batch stays visible during timer')
  await clickText('.global-task-banner', '停止所有任务')
  await waitFor('!document.querySelector(".global-task-banner")')
  await input('select[aria-label="读取类型"]', 'u16')
  await input('select[aria-label="写入类型"]', 'u16')
  await clickText('.read-card', '曲线')
  await assertSingleScreen('820x640: curve mode')
  await clickText('.read-card', '曲线')
  await screenshot('connected-narrow', 820, 640)
  await click('.connection-trigger')
  await expandConnectionSettings()
  await waitFor('Boolean(document.querySelector(\'.connection-dialog input[placeholder="0100"]\')?.getClientRects().length)')
  await assertConnectionDialog('820x640: connected with advanced settings and disconnect')
  assert.equal(await evaluate('Boolean(document.querySelector(".connect-action"))'), false)
  assert.ok(await evaluate('[...document.querySelectorAll(".connection-panel input")].every(input => input.disabled)'))
  await screenshot('connection-s7-connected-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  await field('.read-card', '数量 / 字节', 0)
  await clickText('.read-card', '读取 u16')
  await waitFor('Boolean(document.querySelector(".io-workspace > .studio-alert"))')
  await assertSingleScreen('820x640: validation message')
  await field('.read-card', '数量 / 字节', 3)
  await evaluate('window.__failNext = true')
  await clickText('.read-card', '定时读取')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.ok(await evaluate('document.body.textContent.includes("Mock read failure")'))
  await assertSingleScreen('820x640: read failure')
  console.log('PASS: batch read/write-back, point-table read, stop on read failure')
  await tab('batch')
  await screenshot('connected-desktop', 1440, 1000)
  await click('.connection-trigger')
  await click('.disconnect-action')
  await waitFor('!window.__status.connected')
  assert.equal(await evaluate('Boolean(document.querySelector(".disconnect-action"))'), false)
  assert.equal(await evaluate('document.querySelector(\'input[aria-label="IP 地址"]\').disabled'), false)
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', '通用 Modbus4')
  await clickText('.device-group-list', 'TCP')
  await tab('addresses')
  assert.equal(await evaluate('document.querySelectorAll(".address-example-table thead th").length'), 5)
  assert.equal(await evaluate('document.querySelectorAll(".address-example-table tbody tr").length'), 11)
  assert.ok(await evaluate('document.querySelector("#tool-panel-addresses").textContent.includes("x 是站号，不是功能码")'))
  assert.ok(await evaluate('document.querySelector("#tool-panel-addresses").textContent.includes("不支持 s=")'))
  await screenshot('modbus-addresses', 1440, 1000)
  await screenshot('modbus-addresses-narrow', 820, 640)
  await assertSingleScreen('820x640: Modbus address rules')
  await click('.connection-trigger')
  await expandConnectionSettings()
  await input('input[aria-label="IP 地址"]', '192.0.2.10')
  await assertConnectionDialog('820x640: disconnected Modbus settings')
  await screenshot('connection-modbus-idle', 1440, 1000)
  await click('.connect-action')
  await waitFor('window.__status.protocol === "modbus_tcp" && window.__status.connected')
  await waitFor('document.querySelector(".connection-trigger").getAttribute("aria-expanded") === "false"')
  await click('.connection-trigger')
  assert.equal(await evaluate('Boolean(document.querySelector(".connect-action"))'), false)
  assert.ok(await evaluate('[...document.querySelectorAll(".connection-panel input")].every(input => input.disabled)'))
  assert.ok(await evaluate(`Boolean(document.querySelector('[aria-label="Modbus 字节序"] .n-base-selection--disabled'))`))
  await click('[aria-label="Modbus 字节序"] .n-base-selection')
  assert.equal(await evaluate('Boolean(document.querySelector(".n-base-select-menu"))'), false)
  await screenshot('connection-modbus-connected', 1440, 1000)
  await cdp('Emulation.setDeviceMetricsOverride', { width: 820, height: 640, deviceScaleFactor: 1, mobile: false })
  await assertConnectionDialog('820x640: connected Modbus settings')
  await screenshot('connection-modbus-connected-narrow', 820, 640)
  await click('.connection-dialog .n-card-header__close')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="创建 PLC Rust 示例"]\').textContent.includes("ModbusTcp::new")'))
  assert.ok(await evaluate('document.querySelector(\'[aria-label="创建 PLC Rust 示例"]\').textContent.includes("192.0.2.10")'))
  assert.ok(await evaluate('!document.querySelector(\'[aria-label="创建 PLC Rust 示例"]\').textContent.includes("S7Net")'))
  await clickText('.read-card', '读取 u16')
  assert.ok(await evaluate('document.querySelector(\'[aria-label="当前操作 Rust 示例"]\').textContent.includes(\'read::<u16>("HR0", 1)\')'))
  await tab('batch')
  await input('input[aria-label="批量读取地址"]', 'x=2;IR100')
  await input('input[aria-label="批量读取长度"]', 2)
  await clickText('#tool-panel-batch', '批量读取')
  await waitFor('!document.querySelector(".global-task-banner")')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="批量读取结果"]\').value'), '00 64 00 65')
  assert.equal(await evaluate('document.querySelector(\'[aria-label="确认批量回写"]\').disabled'), true)
  const writesBeforeReadonly = await evaluate('window.__calls.write.length')
  await clickText('#tool-panel-batch', '批量回写')
  assert.equal(await evaluate('window.__calls.write.length'), writesBeforeReadonly)
  await assertSingleScreen('820x640: Modbus read-only batch')
  await screenshot('batch-narrow', 820, 640)
  await click('.connection-trigger')
  await click('.disconnect-action')
  await waitFor('!window.__status.connected')
  await click('.connection-dialog .n-card-header__close')
  await clickText('.device-group-list', '欧姆龙2')
  for (const [label, protocol, client] of [['FINS · TCP', 'omron_fins_tcp', 'OmronFinsTcp'], ['FINS · UDP', 'omron_fins_udp', 'OmronFinsUdp']]) {
    await clickText('.device-group-list', label)
    await click('.connection-trigger')
    await expandConnectionSettings()
    await input('input[aria-label="IP 地址"]', '192.0.2.10')
    await input('input[aria-label="FINS 源节点 SA1"]', 20)
    await input('input[aria-label="FINS 目标节点 DA1"]', 0)
    await input('input[aria-label="FINS 目标网络 · DNA"]', 2)
    const beforeInvalidConnect = await evaluate('window.__calls.connect.length')
    await click('.connect-action')
    await waitFor('Boolean(document.querySelector(".connection-panel .validation-error"))')
    assert.equal(await evaluate('window.__calls.connect.length'), beforeInvalidConnect)
    await assertConnectionDialog(`820x640: ${protocol} route validation`)
    await input('input[aria-label="FINS 目标节点 DA1"]', 10)
    await input('input[aria-label="FINS 源网络 · SNA"]', 1)
    await input('input[aria-label="FINS 源单元 · SA2"]', 254)
    await input('input[aria-label="FINS 网关跳数 · GCT"]', 5)
    await click('[aria-label="FINS 字节序"] .n-base-selection')
    await evaluate(`(() => { const option = [...document.querySelectorAll('.n-base-select-option')].find(option => option.textContent.includes('BADC')); if (!option) throw new Error('Missing BADC option'); option.click(); })()`)
    await click('.connect-action')
    await waitFor(`window.__status.connected && window.__status.protocol === ${JSON.stringify(protocol)} && document.querySelector('.connection-trigger').getAttribute('aria-expanded') === 'false'`)
    assert.deepEqual(await evaluate('window.__calls.connect.at(-1).omron'), { sourceNode: 20, destinationNode: 10, sourceNetwork: 1, destinationNetwork: 2, sourceUnit: 254, destinationUnit: 0, gatewayCount: 5, byteOrder: 'BADC' })
    const creation = await evaluate(`document.querySelector('[aria-label="创建 PLC Rust 示例"]').textContent`)
    assert.ok(creation.includes(client + '::new') && creation.includes('FinsRoute') && creation.includes('source_node: 20') && creation.includes('ByteOrder::BADC'))
    assert.ok(!creation.includes('Melsec'))
    await input('select[aria-label="读取类型"]', 'bool')
    assert.equal(await evaluate(`document.querySelector('input[aria-label="读取地址"]').value`), 'CIO100.0')
    await clickText('.read-card', '读取 bool')
    await waitFor(`window.__calls.read.at(-1)?.dataType === 'bool'`)
    assert.equal(await evaluate('window.__calls.read.at(-1).address'), 'CIO100.0')
    await input('select[aria-label="写入类型"]', 'u16')
    await input('input[aria-label="写入地址"]', 'D200')
    await input('textarea[aria-label="写入值"]', '[1,2]')
    await click('.write-confirmation input')
    const beforeOmronWrite = await evaluate('window.__calls.write.length')
    await clickText('.write-card', '写入数据')
    await waitFor(`window.__calls.write.length === ${beforeOmronWrite + 1}`)
    assert.deepEqual(await evaluate('window.__calls.write.at(-1).values'), ['1', '2'])
    assert.equal(await evaluate('window.__calls.write.at(-1).address'), 'D200')
    assert.ok(await evaluate(`document.querySelector('[aria-label="当前操作 Rust 示例"]').textContent.includes('write_all::<u16>')`))
    await tab('batch')
    await input('input[aria-label="批量读取地址"]', 'D100')
    await input('input[aria-label="批量读取长度"]', 3)
    await clickText('#tool-panel-batch', '批量读取')
    await waitFor('!document.querySelector(".global-task-banner")')
    assert.equal(await evaluate('window.__calls.read.at(-1).dataType'), 'u8')
    assert.equal(await evaluate(`document.querySelector('[aria-label="批量读取结果"]').value`), '41 42 43')
    for (const id of toolIds) { await tab(id); await assertSingleScreen(`820x640: ${protocol} ${id}`) }
    await tab('addresses')
    assert.ok(await evaluate('document.querySelector("#tool-panel-addresses").textContent.includes("E10.100")'))
    await screenshot(`${protocol}-workspace-narrow`, 820, 640)
    await click('.connection-trigger')
    await assertConnectionDialog(`820x640: locked ${protocol} settings`)
    assert.ok(await evaluate('[...document.querySelectorAll(".connection-panel input")].every(input => input.disabled)'))
    assert.equal(await evaluate('Boolean(document.querySelector(".connect-action"))'), false)
    await screenshot(`${protocol}-connected-desktop`, 1440, 1000)
    await screenshot(`${protocol}-connected-narrow`, 820, 640)
    await click('.disconnect-action')
    await waitFor('!window.__status.connected')
    await click('.connection-dialog .n-card-header__close')
  }
  console.log('PASS: Omron TCP/UDP routes, drafts, validation, locked fields, IO, batch bytes, Rust examples and single-screen layout')
  await clickText('.device-group-list', '汇川5')
  const inovanceModels = [['AM / AC / AP', 'AM'], ['AM / AC / AP', 'AC'], ['AM / AC / AP', 'AP'], ['EVO', 'EVO'], ['H3U', 'H3U'], ['H5U', 'H5U'], ['Easy', 'Easy']]
  for (const [index, [label, series]] of inovanceModels.entries()) {
    const iec = ['AM', 'AC', 'AP', 'EVO'].includes(series)
    const address = iec ? 'MW100' : 'D100'
    const bitAddress = iec ? 'MX100.0' : 'M100'
    await clickText('.device-group-list', label)
    await input('select[aria-label="读取类型"]', 'u16')
    assert.equal(await evaluate(`document.querySelector('input[aria-label="读取地址"]').value`), address)
    await click('.connection-trigger')
    await expandConnectionSettings()
    assert.equal(await evaluate(`document.querySelector('input[aria-label="汇川端口，默认 502"]').value`), '502')
    if (!['AC', 'AP'].includes(series)) {
      assert.equal(await evaluate(`document.querySelector('input[aria-label="汇川站号"]').value`), '1')
      assert.ok(await evaluate(`document.querySelector('[aria-label="汇川字节序"]').textContent.includes('CDAB')`))
    } else {
      await click('[aria-label="汇川型号"] .n-base-selection')
      await evaluate(`(() => { const option = [...document.querySelectorAll('.n-base-select-option')].find(option => option.textContent.trim() === ${JSON.stringify(series)}); if (!option) throw new Error('Missing model'); option.click(); })()`)
    }
    await input('input[aria-label="IP 地址"]', '192.0.2.10')
    await input('input[aria-label="汇川站号"]', 9 + index)
    await click('[aria-label="汇川字节序"] .n-base-selection')
    await evaluate(`(() => { const option = [...document.querySelectorAll('.n-base-select-option')].find(option => option.textContent.includes('BADC')); if (!option) throw new Error('Missing BADC'); option.click(); })()`)
    assert.equal(await evaluate('Boolean(document.querySelector(\'input[aria-label="网络号"]\'))'), false)
    await assertConnectionDialog(`820x640: ${series} connection settings`)
    await click('.connect-action')
    await waitFor(`window.__status.connected && window.__status.inovanceSeries === ${JSON.stringify(series)} && document.querySelector('.connection-trigger').getAttribute('aria-expanded') === 'false'`)
    assert.deepEqual(await evaluate('window.__calls.connect.at(-1).inovance'), { series, unitId: 9 + index, byteOrder: 'BADC' })
    assert.equal(await evaluate('window.__calls.connect.at(-1).protocol'), 'inovance_modbus_tcp')
    const creation = await evaluate(`document.querySelector('[aria-label="创建 PLC Rust 示例"]').textContent`)
    assert.ok(creation.includes('InovanceModbusTcp::new') && creation.includes(`InovanceType::${series}`) && creation.includes('ByteOrder::BADC') && creation.includes('192.0.2.10'))
    assert.ok(!creation.includes('Melsec') && !creation.includes('S7Net'))
    await input('select[aria-label="读取类型"]', 'bool')
    assert.equal(await evaluate(`document.querySelector('input[aria-label="读取地址"]').value`), bitAddress)
    const beforeRead = await evaluate('window.__calls.read.length')
    await clickText('.read-card', '读取 bool')
    await waitFor(`window.__calls.read.length === ${beforeRead + 1} && !document.querySelector('.global-task-banner')`)
    assert.equal(await evaluate('window.__calls.read.at(-1).address'), bitAddress)
    assert.ok(await evaluate(`document.querySelector('[aria-label="当前操作 Rust 示例"]').textContent.includes('read::<bool>')`))
    await input('select[aria-label="写入类型"]', 'u16')
    await input('input[aria-label="写入地址"]', iec ? 'MW200' : 'D200')
    await input('textarea[aria-label="写入值"]', '[1,2]')
    if (!await evaluate('document.querySelector(".write-confirmation input").checked')) await click('.write-confirmation input')
    const beforeWrite = await evaluate('window.__calls.write.length')
    await clickText('.write-card', '写入数据')
    await waitFor(`window.__calls.write.length === ${beforeWrite + 1} && !document.querySelector('.global-task-banner')`)
    assert.deepEqual(await evaluate('window.__calls.write.at(-1).values'), ['1', '2'])
    assert.equal(await evaluate('window.__calls.write.at(-1).address'), iec ? 'MW200' : 'D200')
    assert.ok(await evaluate(`document.querySelector('[aria-label="当前操作 Rust 示例"]').textContent.includes('write_all::<u16>')`))
    await tab('batch')
    await input('input[aria-label="批量读取地址"]', address)
    await input('input[aria-label="批量读取长度"]', 3)
    await clickText('#tool-panel-batch', '批量读取')
    await waitFor('!document.querySelector(".global-task-banner")')
    assert.equal(await evaluate('window.__calls.read.at(-1).dataType'), 'u8')
    assert.equal(await evaluate(`document.querySelector('[aria-label="批量读取结果"]').value`), '41 42 43')
    await input('input[aria-label="批量读取地址"]', bitAddress)
    await input('input[aria-label="批量读取长度"]', 1)
    await clickText('#tool-panel-batch', '批量读取')
    await waitFor('!document.querySelector(".global-task-banner")')
    assert.equal(await evaluate('window.__calls.read.at(-1).dataType'), 'bool')
    await input('input[aria-label="批量读取地址"]', iec ? 's=2;IX0.0' : 's=2;X10')
    await clickText('#tool-panel-batch', '批量读取')
    await waitFor('!document.querySelector(".global-task-banner")')
    assert.ok(await evaluate(`document.querySelector('input[aria-label="确认批量回写"]').disabled`))
    for (const id of toolIds) { await tab(id); await assertSingleScreen(`820x640: ${series} ${id}`) }
    await tab('addresses')
    const examples = await evaluate('document.querySelector("#tool-panel-addresses").textContent')
    assert.ok(examples.includes(iec ? 'MX100.0' : 'X10'))
    if (series === 'H3U') assert.ok(examples.includes('C200'))
    if (['AP', 'EVO', 'H3U'].includes(series)) await screenshot(`inovance-${series}-workspace-narrow`, 820, 640)
    await click('.connection-trigger')
    await assertConnectionDialog(`820x640: locked ${series} settings`)
    assert.ok(await evaluate('[...document.querySelectorAll(".connection-panel input")].every(input => input.disabled)'))
    assert.equal(await evaluate('Boolean(document.querySelector(".connect-action"))'), false)
    if (series === 'AP') await screenshot('inovance-ap-connection-narrow', 820, 640)
    await click('.disconnect-action')
    await waitFor('!window.__status.connected')
    await click('.connection-dialog .n-card-header__close')
  }
  await clickText('.device-group-list', 'AM / AC / AP')
  await click('.connection-trigger')
  await expandConnectionSettings()
  assert.ok(await evaluate(`document.querySelector('[aria-label="汇川型号"]').textContent.includes('AP')`))
  assert.equal(await evaluate(`document.querySelector('input[aria-label="汇川站号"]').value`), '11')
  await click('.connection-dialog .n-card-header__close')
  console.log('PASS: Inovance seven models, independent drafts, IPC, IO, batch bytes/bits, read-only protection, Rust samples and 820x640 layout')
  console.log('PASS: Rust auto-navigation and live parameters, buffer parsing/search/edit/write-back, protocol-specific address table and read-only protection')
  assert.deepEqual(errors, [])
  console.log(`PASS: no uncaught browser errors; screenshots: ${output}`)
} finally {
  if (injection) { await cdp('Page.removeScriptToEvaluateOnNewDocument', { identifier: injection.identifier }); await cdp('Page.reload') }
  socket.close()
}
