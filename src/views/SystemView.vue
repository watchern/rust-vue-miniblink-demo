<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { bridge, type SysInfo } from '../bridge'
import { fmtBytes, fmtUptime, pct } from '../format'
import { t } from '../i18n'

const info = ref<SysInfo | null>(null)
const loading = ref(false)
const error = ref('')
const elapsedMs = ref(0)

async function load() {
  loading.value = true
  error.value = ''
  const t0 = performance.now()
  try {
    info.value = await bridge.sysInfo()
    elapsedMs.value = Math.round(performance.now() - t0)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="view">
    <div class="head">
      <div>
        <h1 class="view-title">{{ t('sys.title') }}</h1>
        <p class="view-sub">
          {{ t('sys.sub', { lib: 'sysinfo', cmd: 'sys.info' }) }}
          <span v-if="elapsedMs" class="dim">{{ t('sys.sub.rt', { ms: elapsedMs }) }}</span>
        </p>
      </div>
      <button class="btn" :disabled="loading" @click="load">
        {{ loading ? t('common.reading') : t('common.refresh') }}
      </button>
    </div>

    <div v-if="error" class="card pad err-box">
      <strong>{{ t('common.errorPrefix') }}</strong>{{ error }}
    </div>

    <template v-if="info">
      <div class="section-title">{{ t('sys.section.host') }}</div>
      <div class="card pad">
        <div class="kv"><span class="k">{{ t('sys.kv.host') }}</span><span class="v">{{ info.host }}</span></div>
        <div class="kv"><span class="k">{{ t('sys.kv.os') }}</span><span class="v">{{ info.os }} {{ info.osVersion }}</span></div>
        <div class="kv"><span class="k">{{ t('sys.kv.kernel') }}</span><span class="v">{{ info.kernel }}</span></div>
        <div class="kv"><span class="k">{{ t('sys.kv.arch') }}</span><span class="v">{{ info.arch }}</span></div>
        <div class="kv"><span class="k">{{ t('sys.kv.uptime') }}</span><span class="v">{{ fmtUptime(info.uptimeSec) }}</span></div>
        <div class="kv"><span class="k">{{ t('sys.kv.pid') }}</span><span class="v">{{ info.pid }}</span></div>
      </div>

      <div class="section-title">{{ t('sys.section.cpu') }}</div>
      <div class="card pad">
        <div class="cpu-head">
          <span class="mono">{{ info.cpus[0]?.brand || t('sys.cpu.unknown') }}</span>
          <span class="tag">{{ t('sys.cpu.summary', { n: info.cpuCount, pct: info.cpuUsage.toFixed(1) }) }}</span>
        </div>
        <div class="bar">
          <div class="bar-fill" :style="{ width: info.cpuUsage + '%' }" />
        </div>
        <div class="cores">
          <div v-for="(c, i) in info.cpus.slice(0, 32)" :key="i" class="core" :title="c.brand">
            <div class="core-bar">
              <div class="core-fill" :style="{ height: Math.max(3, c.usage) + '%' }" />
            </div>
            <div class="core-label">{{ Math.round(c.usage) }}</div>
          </div>
        </div>
      </div>

      <div class="section-title">{{ t('sys.section.memory') }}</div>
      <div class="card pad">
        <div class="mem-head">
          <span class="mono">{{ fmtBytes(info.memUsed) }} / {{ fmtBytes(info.memTotal) }}</span>
          <span class="tag">{{ pct(info.memUsed, info.memTotal) }}%</span>
        </div>
        <div class="bar">
          <div class="bar-fill mem" :style="{ width: pct(info.memUsed, info.memTotal) + '%' }" />
        </div>
        <div class="mem-foot dim">{{ t('common.available', { v: fmtBytes(info.memAvailable) }) }}</div>
      </div>

      <div class="section-title">{{ t('sys.section.disk') }}</div>
      <div class="card pad">
        <div v-for="d in info.disks" :key="d.mount" class="disk">
          <div class="disk-head">
            <span class="disk-name">
              {{ d.name }}
              <span class="dim mono">{{ d.mount }}</span>
              <span class="dim">{{ d.fs }}</span>
              <span v-if="d.removable" class="tag warn">{{ t('sys.disk.removable') }}</span>
            </span>
            <span class="mono">{{ pct(d.total - d.available, d.total) }}%</span>
          </div>
          <div class="bar">
            <div class="bar-fill disk" :style="{ width: pct(d.total - d.available, d.total) + '%' }" />
          </div>
          <div class="disk-foot dim">
            {{ t('sys.disk.foot', { avail: fmtBytes(d.available), total: fmtBytes(d.total) }) }}
          </div>
        </div>
        <div v-if="!info.disks.length" class="empty">{{ t('sys.disk.none') }}</div>
      </div>
    </template>

    <div v-else-if="loading" class="card pad empty">{{ t('common.reading') }}</div>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 4px;
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
  background: linear-gradient(90deg, #6aa2f7, #3b6ef5);
  transition: width .35s ease;
}

.bar-fill.mem {
  background: linear-gradient(90deg, #f0a35e, #e07b39);
}

.bar-fill.disk {
  background: linear-gradient(90deg, #7bc86c, #46a13a);
}

.cpu-head,
.mem-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
  font-size: 13px;
}

.mem-foot {
  margin-top: 8px;
  font-size: 12px;
}

.cores {
  display: flex;
  gap: 6px;
  margin-top: 16px;
  height: 54px;
  align-items: flex-end;
}

.core {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  min-width: 0;
}

.core-bar {
  width: 100%;
  height: 38px;
  background: #eceff4;
  border-radius: 3px;
  display: flex;
  align-items: flex-end;
  overflow: hidden;
}

.core-fill {
  width: 100%;
  background: #7ea6f5;
  border-radius: 3px;
  transition: height .3s ease;
}

.core-label {
  font-size: 10px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

.disk {
  padding: 10px 0;
  border-bottom: 1px dashed var(--border);
}

.disk:last-child {
  border-bottom: 0;
  padding-bottom: 0;
}

.disk:first-child {
  padding-top: 0;
}

.disk-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 7px;
  font-size: 13px;
}

.disk-name {
  display: flex;
  align-items: center;
  gap: 7px;
  font-weight: 600;
  min-width: 0;
}

.disk-foot {
  margin-top: 6px;
  font-size: 12px;
}

.err-box {
  color: var(--err);
  background: var(--err-soft);
  border-color: #f5c6c0;
  margin-bottom: 14px;
}
</style>
