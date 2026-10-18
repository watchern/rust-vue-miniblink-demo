<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { bridge, on as onEvent } from '../bridge'
import { t } from '../i18n'

interface LogLine {
  t: string
  text: string
  kind: 'out' | 'in' | 'err' | 'info'
}

const logs = ref<LogLine[]>([])
const echoText = ref('hello from Vue 👋')
const fibN = ref(35)
const fibResult = ref<{ result: string; elapsedUs: number } | null>(null)
const ticking = ref(false)
const busy = ref(false)
const roundTrip = ref<number | null>(null)

const pad = (n: number) => String(n).padStart(2, '0')

function push(text: string, kind: LogLine['kind'] = 'info') {
  const d = new Date()
  logs.value.unshift({
    t: `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(Math.floor(d.getMilliseconds() / 10))}`,
    text,
    kind,
  })
  if (logs.value.length > 240) logs.value.pop()
}

async function sendEcho() {
  busy.value = true
  push(`→ echo  ${echoText.value}`, 'out')
  const t0 = performance.now()
  try {
    const r = await bridge.echo(echoText.value)
    roundTrip.value = Math.round((performance.now() - t0) * 100) / 100
    push(`← echo  ${JSON.stringify(r)}`, 'in')
  } catch (e) {
    push(`✗ ${e instanceof Error ? e.message : String(e)}`, 'err')
  } finally {
    busy.value = false
  }
}

async function runFib() {
  busy.value = true
  push(`→ compute.fib  n=${fibN.value}`, 'out')
  const t0 = performance.now()
  try {
    const r = await bridge.fib(fibN.value)
    fibResult.value = r
    push(
      t('bridge.log.fibIn', {
        n: r.n,
        r: r.result,
        us: r.elapsedUs,
        rt: Math.round((performance.now() - t0) * 100) / 100,
      }),
      'in',
    )
  } catch (e) {
    push(`✗ ${e instanceof Error ? e.message : String(e)}`, 'err')
  } finally {
    busy.value = false
  }
}

async function burst() {
  busy.value = true
  push(t('bridge.log.burstOut'), 'out')
  const t0 = performance.now()
  try {
    await Promise.all(Array.from({ length: 20 }, (_, i) => bridge.echo(`burst-${i}`)))
    const ms = Math.round((performance.now() - t0) * 100) / 100
    push(t('bridge.log.burstIn', { ms }), 'in')
  } catch (e) {
    push(`✗ ${e instanceof Error ? e.message : String(e)}`, 'err')
  } finally {
    busy.value = false
  }
}

async function toggleTick() {
  try {
    if (ticking.value) {
      await bridge.stopTick()
      ticking.value = false
      push('→ event.stop', 'out')
    } else {
      await bridge.startTick(1000)
      ticking.value = true
      push(t('bridge.log.tickStart'), 'out')
    }
  } catch (e) {
    push(`✗ ${e instanceof Error ? e.message : String(e)}`, 'err')
  }
}

function clearLogs() {
  logs.value = []
}

onMounted(() => {
  onEvent('ready', (data) => push(t('bridge.log.ready', { data: JSON.stringify(data) }), 'in'))
  onEvent('tick', (data) => push(`⇠ tick ${JSON.stringify(data)}`, 'in'))
  onEvent('tick-stopped', () => push('⇠ tick-stopped', 'in'))
  push(t('bridge.log.bridgeReady'), 'info')
})
</script>

<template>
  <div class="view">
    <h1 class="view-title">{{ t('bridge.title') }}</h1>
    <p class="view-sub">
      {{ t('bridge.sub', {
        a: 'window.mbQuery',
        b: 'mbOnJsQuery',
        c: 'mbRunJs',
        d: 'window.__mbEmit',
      }) }}
    </p>

    <div class="grid cols-2">
      <div class="card pad">
        <div class="section-title" style="margin-top: 0">{{ t('bridge.sec.echo') }}</div>
        <div class="row">
          <input v-model="echoText" class="input" @keydown.enter="sendEcho" />
          <button class="btn primary" :disabled="busy" @click="sendEcho">{{ t('bridge.btn.send') }}</button>
        </div>
        <div class="hint dim">
          {{ t('bridge.roundTrip') }}
          <strong class="mono">{{ roundTrip === null ? '—' : roundTrip + ' ms' }}</strong>
        </div>

        <div class="section-title">{{ t('bridge.sec.calc') }}</div>
        <div class="row">
          <input v-model.number="fibN" type="number" min="0" max="90" class="input num" />
          <button class="btn" :disabled="busy" @click="runFib">fib(n)</button>
          <button class="btn" :disabled="busy" @click="burst">{{ t('bridge.btn.burst') }}</button>
        </div>
        <div v-if="fibResult" class="result mono">
          fib({{ fibN }}) = {{ fibResult.result }}
          <span class="dim">{{ t('bridge.fibResult', { us: fibResult.elapsedUs }) }}</span>
        </div>
      </div>

      <div class="card pad">
        <div class="section-title" style="margin-top: 0">{{ t('bridge.sec.push') }}</div>
        <div class="row">
          <button class="btn" :class="ticking ? '' : 'primary'" :disabled="busy" @click="toggleTick">
            {{ ticking ? t('bridge.btn.stopPush') : t('bridge.btn.startPush') }}
          </button>
          <span class="tag" :class="ticking ? 'ok' : ''">
            {{ ticking ? t('bridge.pushing') : t('bridge.stopped') }}
          </span>
        </div>
        <p class="hint dim">{{ t('bridge.hint.push1', { api: 'mbRunJs' }) }}</p>
        <p class="hint dim">{{ t('bridge.hint.push2', { ev: 'ready' }) }}</p>
      </div>
    </div>

    <div class="section-title">
      {{ t('bridge.sec.log') }}
      <span class="spacer" />
      <button class="btn sm" @click="clearLogs">{{ t('bridge.btn.clear') }}</button>
    </div>
    <pre class="log"><span
      v-for="(l, i) in logs"
      :key="i"
      :class="l.kind"
    >[{{ l.t }}] {{ l.text }}
</span><span v-if="!logs.length" class="dim">{{ t('bridge.log.empty') }}</span></pre>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.row .input {
  flex: 1;
  min-width: 0;
  user-select: text;
}

.num {
  max-width: 110px;
}

.hint {
  font-size: 12.5px;
  line-height: 1.7;
  margin: 0 0 6px;
}

.result {
  margin-top: 4px;
  padding: 9px 11px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  word-break: break-all;
  line-height: 1.6;
}

.spacer {
  flex: 1;
}

.log .out {
  color: #9ec5ff;
}

.log .err {
  color: #ff8f8f;
}

.log .info {
  color: #98a2b3;
}

.log .in {
  color: #8de08d;
}
</style>
