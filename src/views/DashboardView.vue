<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { bridge, type AppInfo } from '../bridge'
import { fmtBytes, fmtDuration, pct } from '../format'
import { t } from '../i18n'

/**
 * 壳侧对内嵌资源报的是内部格式 `embedded in exe (3 files)`，
 * 这里转成当前语种的友好说法；不是内嵌（开发期读磁盘）就原样显示路径。
 */
const webRootText = computed(() => {
  const w = info.value?.webRoot ?? ''
  return /^embedded in exe/.test(w) ? t('dash.embedded') : w
})

const info = ref<AppInfo | null>(null)
const mem = ref<{ total: number; used: number; available: number } | null>(null)
const loading = ref(true)
const error = ref('')
const elapsed = ref(0)

const ua = navigator.userAgent

// miniblink 的 UA 里 Chrome 版本号可能被抹掉，取不到就退回已知的 132
const chromeVer = ua.match(/Chrome\/([\d.]+)/)?.[1]?.split('.')[0] ?? '132'

// 注意：Vue 3 模板里访问不到 window/screen/navigator（不在模板全局白名单内），
// 所以这些值统一在 setup 里取好再交给模板。
const screenInfo = `${window.screen.width}×${window.screen.height} @${window.devicePixelRatio}x`
const lang = window.navigator.language

const caps: [string, boolean][] = [
  ['ES Module', 'noModule' in document.createElement('script')],
  ['CSS :has()', CSS.supports('selector(:has(a))')],
  ['Container Query', CSS.supports('container-type: inline-size')],
  ['Web Animations', typeof document.createElement('div').animate === 'function'],
  ['OffscreenCanvas', typeof OffscreenCanvas !== 'undefined'],
  ['WebAssembly', typeof WebAssembly !== 'undefined'],
  ['structuredClone', typeof structuredClone === 'function'],
  ['Fetch', typeof fetch === 'function'],
]

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [a, m] = await Promise.all([bridge.appInfo(), bridge.memory()])
    info.value = a
    mem.value = m
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
    elapsed.value = 12
  }
}

onMounted(load)
</script>

<template>
  <div class="view">
    <h1 class="view-title">{{ t('dash.title') }}</h1>
    <p class="view-sub">{{ t('dash.sub') }}</p>

    <div v-if="error" class="card pad err-box">
      <strong>{{ t('common.errorPrefix') }}</strong>{{ error }}
      <button class="btn sm" style="margin-left: 12px" @click="load">{{ t('common.retry') }}</button>
    </div>

    <!-- 三个关键指标 -->
    <div class="grid cols-3">
      <div class="card pad metric">
        <div class="m-label">{{ t('dash.metric.kernel') }}</div>
        <div class="m-value">Chromium {{ chromeVer }}</div>
        <div class="m-sub mono">{{ info?.engine ?? t('common.loading') }}</div>
      </div>
      <div class="card pad metric">
        <div class="m-label">{{ t('dash.metric.port') }}</div>
        <div class="m-value">{{ info ? info.localPort : '—' }}</div>
        <div class="m-sub mono">{{ t('dash.metric.portSub') }}</div>
      </div>
      <div class="card pad metric">
        <div class="m-label">{{ t('dash.metric.proc') }}</div>
        <div class="m-value">{{ info ? `#${info.pid}` : '—' }}</div>
        <div class="m-sub mono">{{ t('dash.metric.procSub', {
          arch: info?.arch ?? '',
          uptime: fmtDuration(info?.uptimeMs ?? 0),
        }) }}</div>
      </div>
    </div>

    <!-- 应用信息 -->
    <div class="section-title">
      {{ t('dash.section.appInfo') }}
      <span class="dim" style="font-weight: 400; text-transform: none">{{ t('dash.section.appInfoTag') }}</span>
    </div>
    <div class="card pad">
      <div v-if="loading && !info" class="empty">{{ t('common.loading') }}</div>
      <template v-else-if="info">
        <div class="kv"><span class="k">{{ t('dash.kv.name') }}</span><span class="v">{{ info.name }}</span></div>
        <div class="kv"><span class="k">{{ t('dash.kv.version') }}</span><span class="v">{{ info.version }}</span></div>
        <div class="kv"><span class="k">{{ t('dash.kv.engine') }}</span><span class="v">{{ info.engine }}</span></div>
        <div class="kv"><span class="k">{{ t('dash.kv.dll') }}</span><span class="v">{{ info.dll }}</span></div>
        <!-- 前端资源：release 内嵌在 exe 里，壳侧会报 "embedded in exe (N files)"，
             翻译成当前语种而不是把内部格式直接甩给用户 -->
        <div class="kv">
          <span class="k">{{ t('dash.kv.webRoot') }}</span>
          <span class="v">{{ webRootText }}</span>
        </div>
        <div class="kv">
          <span class="k">{{ t('dash.kv.userData') }}</span>
          <span class="v">{{ info.userData }}</span>
        </div>
        <div class="kv"><span class="k">{{ t('dash.kv.rustc') }}</span><span class="v">{{ info.rustc }}</span></div>
      </template>
    </div>

    <!-- 内存 -->
    <div class="section-title">{{ t('dash.section.memory') }}</div>
    <div class="card pad">
      <div v-if="!mem" class="empty">{{ t('common.loading') }}</div>
      <template v-else>
        <div class="mem-head">
          <span class="mono">{{ fmtBytes(mem.used) }} / {{ fmtBytes(mem.total) }}</span>
          <span class="tag">{{ pct(mem.used, mem.total) }}%</span>
        </div>
        <div class="bar">
          <div class="bar-fill" :style="{ width: pct(mem.used, mem.total) + '%' }" />
        </div>
        <div class="mem-foot dim">{{ t('common.available', { v: fmtBytes(mem.available) }) }}</div>
      </template>
    </div>

    <!-- 内核能力 -->
    <div class="section-title">
      {{ t('dash.section.caps') }}
      <span class="dim" style="font-weight: 400; text-transform: none">{{ t('dash.section.capsTag') }}</span>
    </div>
    <div class="card pad">
      <div class="ua mono">{{ ua }}</div>
      <div class="caps">
        <span v-for="[name, ok] in caps" :key="String(name)" class="tag" :class="ok ? 'ok' : 'err'">
          {{ name }}
        </span>
      </div>
      <div class="kv"><span class="k">{{ t('dash.kv.screen') }}</span><span class="v">{{ screenInfo }}</span></div>
      <div class="kv"><span class="k">{{ t('dash.kv.lang') }}</span><span class="v">{{ lang }}</span></div>
    </div>
  </div>
</template>

<style scoped>
.metric {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.m-label {
  font-size: 11.5px;
  color: var(--text-3);
  font-weight: 600;
  letter-spacing: .02em;
}

.m-value {
  font-size: 21px;
  font-weight: 650;
  letter-spacing: -.02em;
}

.m-sub {
  font-size: 11.5px;
  color: var(--text-3);
  margin-top: 2px;
}

.mem-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  font-size: 13px;
}

.bar {
  height: 8px;
  border-radius: 5px;
  background: #eceff4;
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  border-radius: 5px;
  background: linear-gradient(90deg, #5b8af7, #3b6ef5);
  transition: width .35s ease;
}

.mem-foot {
  margin-top: 8px;
  font-size: 12px;
}

.ua {
  padding: 9px 11px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text-2);
  line-height: 1.6;
  word-break: break-all;
  user-select: text;
  margin-bottom: 12px;
}

.caps {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 12px;
}

.err-box {
  color: var(--err);
  background: var(--err-soft);
  border-color: #f5c6c0;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
}
</style>
