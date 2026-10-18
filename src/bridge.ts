/**
 * JS ⇄ Rust 桥接层
 *
 * miniblink 在页面全局注入了 mbQuery（见 mb132_x64.dll 内嵌脚本）：
 *   window.mbQuery(customMsg, request, cb)
 * Rust 侧通过 mbOnJsQuery 收到 request，处理后 mbResponseQuery 回包，
 * 前端 cb(customMsg, response) 被触发。这里把它包装成 Promise。
 *
 * 反向（Rust -> JS）：Rust 用 mbRunJs 调用 window.__mbEmit({event,data,ts})，
 * 本模块负责分发给订阅者。
 *
 * 脱离 miniblink（比如直接在浏览器里打开 dist）时自动降级为 mock，
 * 方便单独调试前端。
 */

import { reactive } from 'vue'

export interface NativeResult<T = unknown> {
  ok: boolean
  data?: T
  error?: string
}

export interface EmitPayload {
  event: string
  data: unknown
  ts: number
}

declare global {
  interface Window {
    mbQuery?: (
      customMsg: number,
      request: string,
      cb?: (customMsg: number, response: string) => void,
    ) => void
    __mbEmit?: (p: EmitPayload) => void
  }
}

// ------------------------------------------------------------------ 状态

export const nativeState = reactive({
  ok: false,
  transport: '检测中…',
  lastError: '' as string,
})

// ------------------------------------------------------------------ 事件总线

type Handler = (data: unknown, ts: number) => void
const listeners = new Map<string, Set<Handler>>()

function dispatchEmit(p: EmitPayload) {
  const set = listeners.get(p.event)
  if (!set) return
  for (const h of Array.from(set)) {
    try {
      h(p.data, p.ts)
    } catch (e) {
      console.error('[bridge] 事件处理出错', p.event, e)
    }
  }
}

// miniblink 通过 mbRunJs 调用这个全局函数
if (typeof window !== 'undefined') {
  window.__mbEmit = dispatchEmit
}

export function on(event: string, handler: Handler): () => void {
  let set = listeners.get(event)
  if (!set) {
    set = new Set()
    listeners.set(event, set)
  }
  set.add(handler)
  return () => off(event, handler)
}

export function off(event: string, handler: Handler) {
  listeners.get(event)?.delete(handler)
}

// ------------------------------------------------------------------ 调用

export function invoke<T = unknown>(
  cmd: string,
  payload: unknown = null,
  timeoutMs = 15000,
): Promise<T> {
  const fn = typeof window !== 'undefined' ? window.mbQuery : undefined

  if (typeof fn !== 'function') {
    nativeState.ok = false
    nativeState.transport = 'mock（非 miniblink 环境）'
    return mockInvoke<T>(cmd, payload)
  }

  return new Promise<T>((resolve, reject) => {
    const request = JSON.stringify({ cmd, payload })
    let settled = false

    const timer = setTimeout(() => {
      if (settled) return
      settled = true
      nativeState.lastError = `调用超时: ${cmd}`
      reject(new Error(`调用超时: ${cmd}`))
    }, timeoutMs)

    try {
      fn(0, request, (_msg: number, response: string) => {
        if (settled) return
        settled = true
        clearTimeout(timer)

        nativeState.ok = true
        nativeState.transport = 'mbQuery (miniblink)'

        try {
          const r = JSON.parse(response) as NativeResult<T>
          if (r.ok) resolve(r.data as T)
          else {
            nativeState.lastError = r.error || 'native error'
            reject(new Error(r.error || 'native error'))
          }
        } catch (e) {
          nativeState.lastError = `响应解析失败: ${response}`
          reject(new Error(`响应解析失败: ${response}`))
        }
      })
    } catch (e) {
      if (settled) return
      settled = true
      clearTimeout(timer)
      reject(e instanceof Error ? e : new Error(String(e)))
    }
  })
}

// ------------------------------------------------------------------ mock

const MOCK_DELAY = 90

function mockInvoke<T>(cmd: string, payload: unknown): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    setTimeout(() => {
      const p = (payload ?? {}) as Record<string, unknown>
      try {
        resolve(mockExec(cmd, p) as T)
      } catch (e) {
        reject(e instanceof Error ? e : new Error(String(e)))
      }
    }, MOCK_DELAY)
  })
}

function mockExec(cmd: string, p: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'app.info':
      return {
        name: 'rust-vue-miniblink-demo',
        version: '0.1.0',
        engine: 'miniblink 132 (Chromium 132) [mock]',
        dll: '<mock>',
        webRoot: 'embedded in exe (3 files)',
        userData: '<mock>',
        localPort: 0,
        pid: 0,
        arch: 'x86_64',
        uptimeMs: 1234,
        rustc: 'mock',
        locale: 'zh-CN',
      }
    case 'app.setLocale':
      return { locale: p.locale ?? 'zh-CN' }
    case 'sys.info':
      return {
        host: 'MOCK-HOST',
        os: 'Windows',
        osVersion: '11 (mock)',
        kernel: 'mock',
        arch: 'x86_64',
        cpuCount: 8,
        cpuUsage: 12.5,
        cpus: Array.from({ length: 8 }, (_, i) => ({
          brand: `Mock CPU Core ${i}`,
          freqMHz: 3200,
          usage: 10 + i,
        })),
        memTotal: 16 * 1024 ** 3,
        memUsed: 7 * 1024 ** 3,
        memAvailable: 9 * 1024 ** 3,
        disks: [
          { name: 'C:', mount: 'C:\\', fs: 'NTFS', total: 512 * 1024 ** 3, available: 120 * 1024 ** 3, removable: false },
        ],
        uptimeSec: 36000,
        pid: 0,
      }
    case 'sys.memory':
      return { total: 16 * 1024 ** 3, used: 7 * 1024 ** 3, available: 9 * 1024 ** 3, swapTotal: 0, swapUsed: 0 }
    case 'fs.drives':
      return ['C', 'D'].map((d) => ({ name: `${d}:\\`, path: `${d}:\\` }))
    case 'fs.list':
      return {
        path: String(p.path || 'C:\\mock'),
        parent: null,
        dirs: ['src', 'web', 'vendor'].map((n) => ({ name: n, path: `C:\\mock\\${n}`, isDir: true, size: 0, modified: 0 })),
        files: ['Cargo.toml', 'README.md'].map((n) => ({
          name: n,
          path: `C:\\mock\\${n}`,
          isDir: false,
          size: 1024,
          modified: 1737000000,
        })),
      }
    case 'fs.read':
      return {
        path: String(p.path || ''),
        size: 42,
        truncated: false,
        content: '// mock 文件内容\nconsole.log("hello from mock")\n',
      }
    case 'fs.write':
      return { path: String(p.path || ''), bytes: String(p.content || '').length }
    case 'echo':
      return { echo: p.message ?? null, at: Date.now() }
    case 'compute.fib': {
      const n = Number(p.n ?? 30)
      const t0 = performance.now()
      let a = 0n
      let b = 1n
      for (let i = 2n; i <= BigInt(n); i++) {
        const t = a + b
        a = b
        b = t
      }
      return { n, result: b.toString(), elapsedUs: Math.round((performance.now() - t0) * 1000) }
    }
    case 'win.maximize':
      return { maximized: true, action: 'maximize', ok: true }
    case 'win.minimize':
    case 'win.close':
    case 'win.drag':
    case 'win.title':
      return { action: cmd.replace('win.', ''), ok: true, title: p.title }
    case 'win.getSize':
      // 浏览器里调试 mock：给个设计尺寸，resize 把手就能拖
      return { w: 1180, h: 760, ok: true }
    case 'win.resize':
      return { w: Number(p.w ?? 1180), h: Number(p.h ?? 760), ok: true }
    case 'event.start':
      return { started: true, intervalMs: p.intervalMs ?? 1000 }
    case 'event.stop':
      return { started: false }
    default:
      throw new Error(`mock: 未知命令 ${cmd}`)
  }
}

// ------------------------------------------------------------------ 便捷 API

export interface AppInfo {
  name: string
  version: string
  engine: string
  dll: string
  webRoot: string
  /** 运行时数据目录（LocalStorage / cookies / 缓存），壳侧在 %LOCALAPPDATA% 下 */
  userData: string
  localPort: number
  pid: number
  arch: string
  uptimeMs: number
  rustc: string
  locale: string
}

export interface SysInfo {
  host: string
  os: string
  osVersion: string
  kernel: string
  arch: string
  cpuCount: number
  cpuUsage: number
  cpus: { brand: string; freqMHz: number; usage: number }[]
  memTotal: number
  memUsed: number
  memAvailable: number
  disks: { name: string; mount: string; fs: string; total: number; available: number; removable: boolean }[]
  uptimeSec: number
  pid: number
}

export interface FsEntry {
  name: string
  path: string
  isDir: boolean
  size: number
  modified: number
}

export interface FsList {
  path: string
  parent: string | null
  dirs: FsEntry[]
  files: FsEntry[]
}

export const bridge = {
  invoke,
  on,
  off,

  appInfo: () => invoke<AppInfo>('app.info'),
  // 把语种同步给壳，让 IPC 返回的错误文案也跟随界面语言
  setLocale: (locale: string) => invoke<{ locale: string }>('app.setLocale', { locale }),
  sysInfo: () => invoke<SysInfo>('sys.info'),
  memory: () => invoke<{ total: number; used: number; available: number }>('sys.memory'),

  drives: () => invoke<{ name: string; path: string }[]>('fs.drives'),
  listDir: (path: string) => invoke<FsList>('fs.list', { path }),
  readFile: (path: string) => invoke<{ path: string; size: number; truncated: boolean; content: string }>('fs.read', { path }),
  writeFile: (path: string, content: string) => invoke<{ path: string; bytes: number }>('fs.write', { path, content }),

  echo: (message: unknown) => invoke<{ echo: unknown; at: number }>('echo', { message }),
  fib: (n: number) => invoke<{ n: number; result: string; elapsedUs: number }>('compute.fib', { n }),
  hash: (rounds: number) => invoke<{ rounds: number; hash: string; elapsedUs: number }>('compute.hash', { rounds }),

  startTick: (intervalMs = 1000) => invoke<{ started: boolean; intervalMs: number }>('event.start', { intervalMs }),
  stopTick: () => invoke<{ started: boolean }>('event.stop'),

  windowMinimize: () => invoke('win.minimize'),
  windowMaximize: () => invoke<{ maximized: boolean }>('win.maximize'),
  windowClose: () => invoke('win.close'),
  windowDrag: () => invoke('win.drag'),
  windowSize: () => invoke<{ w: number; h: number }>('win.getSize'),
  windowResize: (w: number, h: number) => invoke<{ w: number; h: number }>('win.resize', { w, h }),
  setTitle: (title: string) => invoke('win.title', { title }),
}
