import { mkdtemp, writeFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { createDeviceConnection, deviceGroups } from '../src/deviceCatalog.ts'
import { availableDataTypes, defaultAddress } from '../src/types.ts'
import { generateRustExample } from '../src/rustExamples.ts'

const root = fileURLToPath(new URL('../../', import.meta.url)).replaceAll('\\', '/')
const folder = await mkdtemp(join(tmpdir(), 'plc-rust-examples-'))
await mkdir(join(folder, 'src'))
const sources = []
for (const device of deviceGroups.flatMap(group => group.devices)) {
  for (const protocol of device.protocols) {
    for (const series of device.inovanceSeries ?? [undefined]) {
      const config = { ...createDeviceConnection(device.id), protocol }
      if (series) config.inovance.series = series
      if (protocol === 's7') { config.localTsap = '0100'; config.remoteTsap = '0301' }
      sources.push(generateRustExample(config).source)
      for (const { value: dataType } of availableDataTypes(protocol)) {
        const address = defaultAddress(protocol, dataType, config.inovance.series)
        sources.push(generateRustExample(config, { kind: 'read', request: { address, dataType, ...(dataType === 's7_string' ? {} : { length: 4 }) } }).source)
        if (dataType !== 'raw_string' || protocol !== 's7') {
          const value = dataType === 'bool' ? 'true' : dataType.includes('string') ? 'PLC"\\\n中文' : dataType === 'u64' ? '18446744073709551615' : '123'
          sources.push(generateRustExample(config, { kind: 'write', request: { address, dataType, values: [value], mode: 'single', confirmed: false } }).source)
          if (!dataType.includes('string')) sources.push(generateRustExample(config, { kind: 'write', request: { address, dataType, values: [value, value], mode: 'array', confirmed: false } }).source)
        }
      }
    }
  }
}
for (const id of ['s7-1200', 'omron-fins-tcp', 'omron-fins-udp', 'inovance-am', 'inovance-evo', 'inovance-h3u', 'inovance-h5u', 'inovance-easy']) {
  for (const encoding of ['utf-8', 'ascii', 'utf-16le', 'utf-16be']) {
    const config = createDeviceConnection(id)
    for (const reverse of [false, true]) sources.push(generateRustExample(config, { kind: 'read', request: { address: defaultAddress(config.protocol, 'u8', config.inovance.series), dataType: 'u8', length: 4 }, text: { encoding, reverse } }).source)
  }
}
await writeFile(join(folder, 'Cargo.toml'), `[package]\nname = "plc-example-compile-check"\nversion = "0.0.0"\nedition = "2024"\n[dependencies]\nrs_appliaction = { path = ${JSON.stringify(root)} }\nserialport = { version = "4", default-features = false }\n`)
await writeFile(join(folder, 'src/lib.rs'), '#![allow(dead_code, unused_mut)]\n' + sources.map((source, index) => `mod example_${index} {\n${source}\n}`).join('\n'))
const result = spawnSync('cargo', ['check', '--offline', '--manifest-path', join(folder, 'Cargo.toml'), '--target-dir', join(root, 'demo/src-tauri/target/example-check')], { stdio: 'inherit' })
if (result.error) throw result.error
if (result.status !== 0) process.exit(result.status ?? 1)
console.log(`PASS: ${sources.length} generated Rust examples compile; none were executed. Fixtures: ${folder}`)
