<script setup lang="ts">
import { ref } from 'vue'
import { bridge } from '../bridge'
import { t, locale, otherLocale, toggleLocale, LOCALE_LABEL } from '../i18n'

const props = defineProps<{ title: string; subtitle: string }>()
const emit = defineEmits<{ (e: 'toggle-locale'): void }>()
const maximized = ref(false)

// 标题栏拖动：交给 Rust 侧发送 WM_NCLBUTTONDOWN 实现无边框拖拽
function onDrag(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('.win-btn')) return
  if ((e.target as HTMLElement).closest('.lang-btn')) return
  if (e.button !== 0) return
  bridge.windowDrag()
}

/**
 * 语种切换。切换动作交给父组件统一处理（它还要同步窗口标题），
 * 这里只负责按钮本身的文案与状态。
 */
function onToggleLocale(e: MouseEvent) {
  e.stopPropagation()
  emit('toggle-locale')
}

async function minimize() {
  await bridge.windowMinimize().catch(() => {})
}

async function toggleMax() {
  try {
    maximized.value = await bridge.windowMaximize()
  } catch {
    /* ignore */
  }
}

async function close() {
  await bridge.windowClose().catch(() => {})
}
</script>

<template>
  <header class="titlebar" @mousedown="onDrag">
    <div class="brand">
      <svg class="logo" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
        <rect x="2" y="3.5" width="24" height="24" rx="3" fill="#3b6ef5" />
        <path d="M6.6 15.4 10.4 9.6l2.5 4 1.9-2.6 2.6 4.4" stroke="#fff" stroke-width="1.7"
          stroke-linecap="round" stroke-linejoin="round" fill="none" />
      </svg>
      <div class="titles">
        <span class="t1">{{ props.title }}</span>
        <span class="t2">{{ props.subtitle }}</span>
      </div>
    </div>

    <div class="center">
      <span class="pill">miniblink</span>
      <span class="sep">·</span>
      <span class="pill">Vue 3</span>
      <span class="sep">·</span>
      <span class="pill">Rust</span>
    </div>

    <div class="win-btns">
      <!-- 语种切换：显示"另一种语言"的写法，点击后切过去 -->
      <button
        class="win-btn lang-btn"
        :title="t('lang.switch')"
        :aria-label="t('lang.switch')"
        :aria-pressed="locale === 'en-US'"
        @mousedown.stop
        @click="onToggleLocale"
      >
        <span class="lang-text">{{ LOCALE_LABEL[otherLocale] }}</span>
      </button>

      <button class="win-btn" :title="t('win.minimize')" @click="minimize">
        <svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 5h10" stroke="currentColor" stroke-width="1.2"/></svg>
      </button>
      <button class="win-btn" :title="maximized ? t('win.restore') : t('win.maximize')" @click="toggleMax">
        <svg v-if="!maximized" viewBox="0 0 10 10" width="10" height="10">
          <rect x="0.6" y="0.6" width="8.8" height="8.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
        </svg>
        <svg v-else viewBox="0 0 10 10" width="10" height="10">
          <rect x="0.6" y="2.4" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
          <path d="M2.6 2.4V1.1a.5.5 0 0 1 .5-.5H9a.6.6 0 0 1 .6.6v5.9a.5.5 0 0 1-.5.5H7" fill="none" stroke="currentColor" stroke-width="1.2"/>
        </svg>
      </button>
      <button class="win-btn close" :title="t('win.close')" @click="close">
        <svg viewBox="0 0 10 10" width="10" height="10">
          <path d="M0.7 0.7l8.6 8.6M9.3 0.7L0.7 9.3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  height: 30px;
  flex: 0 0 30px;
  padding: 0 4px 0 4px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
}

.brand {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}

.logo {
  display: block;
  border-radius: 5px;
}

.titles {
  display: flex;
  align-items: baseline;
  gap: 4px;
  min-width: 0;
}

.t1 {
  font-size: 12.5px;
  font-weight: 650;
  white-space: nowrap;
}

.t2 {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
}

.center {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  color: var(--text-3);
  font-size: 11.5px;
}

.pill {
  padding: 2px 8px;
  border-radius: 20px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  font-weight: 600;
  letter-spacing: .02em;
}

.sep {
  opacity: .5;
}

.win-btns {
  display: flex;
  align-items: center;
  gap: 2px;
}

.win-btn {
  display: grid;
  place-items: center;
  width: 34px;
  height: 30px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  transition: background .12s, color .12s;
}

.win-btn:hover {
  background: var(--hover-bg);
  color: var(--text);
}

.win-btn.close:hover {
  background: #e5484d;
  color: #fff;
}

/* 语种切换：比窗口按钮宽一点，容纳「中」/「EN」两个字 */
.lang-btn {
  width: 40px;
  margin-right: 6px;
  border: 1px solid var(--border);
}

.lang-btn:hover {
  background: var(--accent-soft);
  color: var(--accent);
  border-color: #b9cdfb;
}

.lang-text {
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: .02em;
}
</style>
