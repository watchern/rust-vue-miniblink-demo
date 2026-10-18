/**
 * 极简 i18n（零第三方依赖）
 *
 * 为什么不用 vue-i18n：这个 demo 的卖点是「miniblink 单 DLL + Rust 壳」，
 * 产物只有 index.html + 一个 js。为了两个语种引入完整的 vue-i18n 不划算，
 * 而且壳侧（本文件）需要同步一份同样的文案表过去（见 src-rs/i18n.rs），
 * 两边都要能独立工作。极简实现让「一份文案表、两处消费」的成本最低。
 *
 * 用法：
 *   import { t, locale, setLocale, toggleLocale, LOCALE_LABEL } from './i18n'
 *   t('nav.dashboard')            // 取当前语种文案
 *   t('files.itemCount', { n: 3 })// 带参数插值
 *   setLocale('en-US')             // 切换（会写 localStorage 并同步 <html lang>）
 */

import { ref, computed } from 'vue'

export type Locale = 'zh-CN' | 'en-US'
export const LOCALES: Locale[] = ['zh-CN', 'en-US']
const STORAGE_KEY = 'mbvue.locale'

/** 文案表。key 用点分层级，方便按模块核对覆盖率。 */
const DICT: Record<Locale, Record<string, string>> = {
  'zh-CN': {
    // ---- 壳 / 导航 ----
    'app.title': 'miniblink + Vue 3 + Rust',
    'nav.dashboard': '概览',
    'nav.dashboard.hint': '内核与环境',
    'nav.system': '系统信息',
    'nav.system.hint': '调用 Rust 读取',
    'nav.files': '文件浏览',
    'nav.files.hint': '本地磁盘读写',
    'nav.bridge': '双向通信',
    'nav.bridge.hint': 'JS ⇄ Rust',
    'nav.about': '关于',
    'nav.about.hint': '架构说明',
    'sub.dashboard': '内核与运行环境',
    'sub.system': 'Rust 读取的主机信息',
    'sub.files': '本地磁盘读写',
    'sub.bridge': 'JS ⇄ Rust 实时调用',
    'sub.about': '项目结构与技术要点',

    'status.connected': '已连接',
    'status.disconnected': '未连接',
    'status.detecting': '检测中…',

    // ---- 标题栏按钮 ----
    'win.minimize': '最小化',
    'win.restore': '向下还原',
    'win.maximize': '最大化',
    'win.close': '关闭',
    'lang.switch': '切换到 English',
    'lang.current': '当前语种：简体中文',
    'lang.short': 'EN',

    // ---- 通用 ----
    'common.retry': '重试',
    'common.refresh': '刷新',
    'common.loading': '加载中…',
    'common.reading': '正在读取…',
    'common.errorPrefix': '调用失败：',
    'common.errorPrefixShort': '出错：',
    'common.available': '可用 {v}',
    'common.empty': '空',

    // ---- 概览 ----
    'dash.title': '概览',
    'dash.sub': '页面跑在 miniblink 内核里，数据由 Rust 侧通过 mbQuery 实时提供。',
    'dash.metric.kernel': '渲染内核',
    'dash.metric.port': '本地服务端口',
    'dash.metric.portSub': '127.0.0.1 · 由 Rust 内置 HTTP 提供',
    'dash.metric.proc': '进程',
    'dash.metric.procSub': '{arch} · 运行 {uptime}',
    'dash.section.appInfo': '应用信息',
    'dash.section.appInfoTag': '（Rust → JS）',
    'dash.kv.name': '名称',
    'dash.kv.version': '版本',
    'dash.kv.engine': '内核',
    'dash.kv.dll': 'DLL 路径',
    'dash.kv.webRoot': '前端资源',
    'dash.kv.userData': '用户数据目录',
    'dash.embedded': '已内嵌于 exe',
    'dash.kv.rustc': '编译器',
    'dash.section.memory': '内存占用',
    'dash.section.caps': '内核能力探测',
    'dash.section.capsTag': '（纯前端）',
    'dash.kv.screen': '屏幕',
    'dash.kv.lang': '语言',
    'dash.tag.rustToJs': 'Rust → JS',
    'dash.tag.frontend': '纯前端',

    // ---- 系统信息 ----
    'sys.title': '系统信息',
    'sys.sub': '由 Rust 侧 {lib} 采集，经 {cmd} 命令返回。',
    'sys.sub.rt': '（本次往返 {ms} ms）',
    'sys.section.host': '主机',
    'sys.kv.host': '主机名',
    'sys.kv.os': '操作系统',
    'sys.kv.kernel': '内核版本',
    'sys.kv.arch': '架构',
    'sys.kv.uptime': '已运行',
    'sys.kv.pid': '进程 PID',
    'sys.section.cpu': '处理器',
    'sys.cpu.unknown': '未知',
    'sys.cpu.summary': '{n} 核 · 占用 {pct}%',
    'sys.section.memory': '内存',
    'sys.section.disk': '磁盘',
    'sys.disk.removable': '可移动',
    'sys.disk.foot': '{avail} 可用 / 共 {total}',
    'sys.disk.none': '未检测到磁盘',

    // ---- 文件浏览 ----
    'files.title': '文件浏览',
    'files.sub': '读写本地磁盘，全部经过 Rust 侧的 {cmds} 命令。',
    'files.btn.up': '↑ 上一级',
    'files.btn.writeSample': '写测试文件',
    'files.ph.path': '输入路径后回车',
    'files.pane.list': '文件列表',
    'files.pane.preview': '文件预览',
    'files.emptyDir': '空目录',
    'files.noFile': '未选择文件',
    'files.clickHint': '点击左侧文件查看内容',
    'files.truncated': '内容已截断',
    'files.saving': '保存中…',
    'files.save': '保存到磁盘',
    'files.toast.written': '已写入 {n} 字节',
    'files.toast.created': '已创建 {p}',
    'files.sampleContent': '由 Vue 前端写入\n时间: {time}\n',

    // ---- 双向通信 ----
    'bridge.title': '双向通信',
    'bridge.sub': 'JS → Rust 走 {a}（{b}）；Rust → JS 走 {c} → {d}。',
    'bridge.sec.echo': 'JS → Rust：echo',
    'bridge.sec.calc': 'JS → Rust：计算',
    'bridge.sec.push': 'Rust → JS：事件推送',
    'bridge.btn.send': '发送',
    'bridge.btn.burst': '并发 ×20',
    'bridge.roundTrip': '单次往返',
    'bridge.fibResult': '· Rust 计算 {us} µs',
    'bridge.btn.startPush': '开始推送（每秒）',
    'bridge.btn.stopPush': '停止推送',
    'bridge.pushing': '推送中',
    'bridge.stopped': '已停止',
    'bridge.hint.push1': 'Rust 侧起后台线程定时采样内存/CPU，用 {api} 推给，前端不需要轮询。',
    'bridge.hint.push2': '打开页面时 Rust 还会主动推一次 {ev} 事件（在文档就绪 350ms 后）。',
    'bridge.sec.log': '调用日志',
    'bridge.btn.clear': '清空',
    'bridge.log.empty': '暂无记录',
    'bridge.log.burstOut': '→ 并发 20 次 echo',
    'bridge.log.burstIn': '← 20 次并发调用完成，总耗时 {ms} ms',
    'bridge.log.fibIn': '← fib({n}) = {r}  [Rust 耗时 {us} µs / 往返 {rt} ms]',
    'bridge.log.tickStart': '→ event.start  每秒由 Rust 主动推送 tick',
    'bridge.log.ready': '⇠ ready（Rust 主动推送）{data}',
    'bridge.log.bridgeReady': '桥接层就绪，等待 Rust 侧 ready 事件…',

    // ---- 关于 ----
    'about.title': '关于这个 Demo',
    'about.sub': 'miniblink(Chromium 132) + Vue 3 + Rust 的桌面应用骨架。',
    'about.section.layers': '分层',
    'about.section.flow': '通信链路',
    'about.section.cmds': '可用命令',
    'about.section.tree': '目录结构',
    'about.section.ops': '常用操作',
    'about.layer.ui': 'UI 层',
    'about.layer.uiDesc': '负责全部界面与交互，产物是标准静态文件（build_dist/）',
    'about.layer.render': '渲染层',
    'about.layer.renderDesc': 'Chromium 132 内核的单 DLL，提供窗口 + WebView + JS 互操作',
    'about.layer.native': '原生层',
    'about.layer.nativeDesc': '无边框窗口、内置 HTTP 服务、文件系统、系统信息、命令分发',
    'about.flow.note': '两条链路互不依赖：入站是「请求-响应」，出站是「推送-订阅」。',
    'about.op.dev': '开发（前端热更新）',
    'about.op.build': '构建',
    'about.op.dist': '打安装包',
    'about.op.refetch': '重新获取内核',

    // ---- 安装包（About 页新增一节）----
    'about.section.installer': '安装包',
    'about.installer.desc': '用 NSIS 打单文件安装器 {file}，安装器界面只含中英文；安装后目录仅 {files}（{size}）。',
    'about.installer.trimmed': '精简掉了不参与运行的文件：',
    'about.cmd.win': '无边框窗口控制',

    // ---- 通用单位（format.ts 用）----
    'unit.day': '天',
    'unit.hour': '小时',
    'unit.minute': '分钟',
    'unit.secShort': '秒',
  },

  'en-US': {
    // ---- Shell / nav ----
    'app.title': 'miniblink + Vue 3 + Rust',
    'nav.dashboard': 'Overview',
    'nav.dashboard.hint': 'Kernel & env',
    'nav.system': 'System',
    'nav.system.hint': 'Read by Rust',
    'nav.files': 'Files',
    'nav.files.hint': 'Local disk I/O',
    'nav.bridge': 'Bridge',
    'nav.bridge.hint': 'JS ⇄ Rust',
    'nav.about': 'About',
    'nav.about.hint': 'Architecture',
    'sub.dashboard': 'Kernel and runtime',
    'sub.system': 'Host info read by Rust',
    'sub.files': 'Local disk read / write',
    'sub.bridge': 'Live JS ⇄ Rust calls',
    'sub.about': 'Layout and key techniques',

    'status.connected': 'Connected',
    'status.disconnected': 'Not connected',
    'status.detecting': 'Detecting…',

    // ---- Title bar buttons ----
    'win.minimize': 'Minimize',
    'win.restore': 'Restore',
    'win.maximize': 'Maximize',
    'win.close': 'Close',
    'lang.switch': 'Switch to 简体中文',
    'lang.current': 'Current language: English',
    'lang.short': '中',

    // ---- Common ----
    'common.retry': 'Retry',
    'common.refresh': 'Refresh',
    'common.loading': 'Loading…',
    'common.reading': 'Reading…',
    'common.errorPrefix': 'Call failed: ',
    'common.errorPrefixShort': 'Error: ',
    'common.available': '{v} available',
    'common.empty': 'Empty',

    // ---- Overview ----
    'dash.title': 'Overview',
    'dash.sub': 'The page runs inside the miniblink kernel; data is served live by Rust over mbQuery.',
    'dash.metric.kernel': 'Render kernel',
    'dash.metric.port': 'Local server port',
    'dash.metric.portSub': '127.0.0.1 · served by the built-in Rust HTTP server',
    'dash.metric.proc': 'Process',
    'dash.metric.procSub': '{arch} · up {uptime}',
    'dash.section.appInfo': 'App info',
    'dash.section.appInfoTag': ' (Rust → JS)',
    'dash.kv.name': 'Name',
    'dash.kv.version': 'Version',
    'dash.kv.engine': 'Kernel',
    'dash.kv.dll': 'DLL path',
    'dash.kv.webRoot': 'Web assets',
    'dash.kv.userData': 'User data dir',
    'dash.embedded': 'embedded in exe',
    'dash.kv.rustc': 'Compiler',
    'dash.section.memory': 'Memory usage',
    'dash.section.caps': 'Kernel capabilities',
    'dash.section.capsTag': ' (frontend only)',
    'dash.kv.screen': 'Screen',
    'dash.kv.lang': 'Language',
    'dash.tag.rustToJs': 'Rust → JS',
    'dash.tag.frontend': 'Frontend',

    // ---- System ----
    'sys.title': 'System info',
    'sys.sub': 'Collected by {lib} on the Rust side, returned via the {cmd} command.',
    'sys.sub.rt': ' (round trip {ms} ms)',
    'sys.section.host': 'Host',
    'sys.kv.host': 'Host name',
    'sys.kv.os': 'Operating system',
    'sys.kv.kernel': 'Kernel version',
    'sys.kv.arch': 'Architecture',
    'sys.kv.uptime': 'Uptime',
    'sys.kv.pid': 'Process PID',
    'sys.section.cpu': 'Processor',
    'sys.cpu.unknown': 'Unknown',
    'sys.cpu.summary': '{n} cores · {pct}% used',
    'sys.section.memory': 'Memory',
    'sys.section.disk': 'Disks',
    'sys.disk.removable': 'Removable',
    'sys.disk.foot': '{avail} available / {total} total',
    'sys.disk.none': 'No disk detected',

    // ---- Files ----
    'files.title': 'Files',
    'files.sub': 'Read and write local disk, all through the Rust {cmds} commands.',
    'files.btn.up': '↑ Up one level',
    'files.btn.writeSample': 'Write test file',
    'files.ph.path': 'Type a path and press Enter',
    'files.pane.list': 'File list',
    'files.pane.preview': 'Preview',
    'files.emptyDir': 'Empty directory',
    'files.noFile': 'No file selected',
    'files.clickHint': 'Click a file on the left to view its content',
    'files.truncated': 'Content truncated',
    'files.saving': 'Saving…',
    'files.save': 'Save to disk',
    'files.toast.written': 'Wrote {n} bytes',
    'files.toast.created': 'Created {p}',
    'files.sampleContent': 'Written by the Vue frontend\nTime: {time}\n',

    // ---- Bridge ----
    'bridge.title': 'Two-way bridge',
    'bridge.sub': 'JS → Rust goes through {a} ({b}); Rust → JS goes through {c} → {d}.',
    'bridge.sec.echo': 'JS → Rust: echo',
    'bridge.sec.calc': 'JS → Rust: compute',
    'bridge.sec.push': 'Rust → JS: event push',
    'bridge.btn.send': 'Send',
    'bridge.btn.burst': 'Burst ×20',
    'bridge.roundTrip': 'Round trip',
    'bridge.fibResult': '· Rust compute {us} µs',
    'bridge.btn.startPush': 'Start push (1s)',
    'bridge.btn.stopPush': 'Stop push',
    'bridge.pushing': 'Pushing',
    'bridge.stopped': 'Stopped',
    'bridge.hint.push1': 'A Rust background thread samples memory/CPU on a timer and pushes via {api}; the frontend never polls.',
    'bridge.hint.push2': 'On page load Rust also pushes a {ev} event once (350ms after the document is ready).',
    'bridge.sec.log': 'Call log',
    'bridge.btn.clear': 'Clear',
    'bridge.log.empty': 'No records yet',
    'bridge.log.burstOut': '→ 20 concurrent echo',
    'bridge.log.burstIn': '← 20 concurrent calls done in {ms} ms',
    'bridge.log.fibIn': '← fib({n}) = {r}  [Rust {us} µs / round trip {rt} ms]',
    'bridge.log.tickStart': '→ event.start  Rust pushes a tick every second',
    'bridge.log.ready': '⇠ ready (pushed by Rust) {data}',
    'bridge.log.bridgeReady': 'Bridge ready, waiting for the ready event from Rust…',

    // ---- About ----
    'about.title': 'About this demo',
    'about.sub': 'A desktop app skeleton built on miniblink (Chromium 132) + Vue 3 + Rust.',
    'about.section.layers': 'Layers',
    'about.section.flow': 'Communication paths',
    'about.section.cmds': 'Available commands',
    'about.section.tree': 'Directory layout',
    'about.section.ops': 'Common operations',
    'about.layer.ui': 'UI layer',
    'about.layer.uiDesc': 'Owns the whole UI and interactions; output is plain static files (build_dist/)',
    'about.layer.render': 'Render layer',
    'about.layer.renderDesc': 'A single DLL holding the Chromium 132 kernel; provides window + WebView + JS interop',
    'about.layer.native': 'Native layer',
    'about.layer.nativeDesc': 'Borderless window, built-in HTTP server, file system, system info, command dispatch',
    'about.flow.note': 'The two paths are independent: inbound is request/response, outbound is push/subscribe.',
    'about.op.dev': 'Develop (frontend HMR)',
    'about.op.build': 'Build',
    'about.op.dist': 'Package installer',
    'about.op.refetch': 'Re-fetch the kernel',

    'about.section.installer': 'Installer',
    'about.installer.desc': 'A single-file NSIS installer {file}; the installer UI ships Chinese and English only. After install the folder holds just {files} ({size}).',
    'about.installer.trimmed': 'Files trimmed out because they never load at runtime:',
    'about.cmd.win': 'Borderless window control',

    // ---- Units (used by format.ts) ----
    'unit.day': 'd',
    'unit.hour': 'h',
    'unit.minute': 'min',
    'unit.secShort': 's',
  },
}

/** 首次进入时的语种：localStorage > 浏览器语言（中文环境给 zh，其余 en）。 */
function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved === 'zh-CN' || saved === 'en-US') return saved
  } catch {
    /* 隐私模式下 localStorage 可能抛异常，忽略 */
  }
  const nav = (typeof navigator !== 'undefined' ? navigator.language : 'en') || 'en'
  return /^zh/i.test(nav) ? 'zh-CN' : 'en-US'
}

const current = ref<Locale>(detect())

/** 当前语种（响应式）。 */
export const locale = computed(() => current.value)

/** 另一侧语种（切换按钮上显示"切到哪种"）。 */
export const otherLocale = computed<Locale>(() => (current.value === 'zh-CN' ? 'en-US' : 'zh-CN'))

export const isZh = computed(() => current.value === 'zh-CN')

/** 切换按钮上显示的文字：永远显示"另一种语言"的写法。 */
export const LOCALE_LABEL: Record<Locale, string> = {
  'zh-CN': 'EN',
  'en-US': '中',
}

function interpolate(tpl: string, params?: Record<string, unknown>): string {
  if (!params) return tpl
  return tpl.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m))
}

/**
 * 取文案。params 做 {name} 插值。
 * 找不到 key 时回落到中文表，再回落到 key 本身 —— 宁可显示 key 也不要空白。
 */
export function t(key: string, params?: Record<string, unknown>): string {
  const table = DICT[current.value]
  let s = table[key]
  if (s === undefined) s = DICT['zh-CN'][key]
  if (s === undefined) {
    if (typeof console !== 'undefined') console.warn(`[i18n] 缺少文案: ${key}`)
    return key
  }
  return interpolate(s, params)
}

/** 切换到指定语种：更新响应式状态 + 落盘 + 同步 <html lang>。 */
export function setLocale(next: Locale) {
  current.value = next
  try {
    localStorage.setItem(STORAGE_KEY, next)
  } catch {
    /* 忽略 */
  }
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('lang', next)
  }
}

/** 中英互切。返回切换后的语种，便于调用方做联动（如同步窗口标题）。 */
export function toggleLocale(): Locale {
  setLocale(otherLocale.value)
  return current.value
}

/** 供覆盖率自检用：列出某个语种缺失的 key。 */
export function missingKeys(l: Locale): string[] {
  const base = Object.keys(DICT['zh-CN'])
  return base.filter((k) => DICT[l][k] === undefined)
}

if (typeof document !== 'undefined') {
  document.documentElement.setAttribute('lang', current.value)
}
