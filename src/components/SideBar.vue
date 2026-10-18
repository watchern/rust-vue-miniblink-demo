<script setup lang="ts">
import { computed } from 'vue'
import { nativeState } from '../bridge'
import { t, otherLocale, LOCALE_LABEL } from '../i18n'

const props = defineProps<{ current: string }>()
const emit = defineEmits<{
  (e: 'change', id: string): void
  (e: 'toggle-locale'): void
}>()

interface NavItem {
  id: string
  icon: string
}

// label / hint 走 i18n（key = nav.<id>[.hint]），避免在这里维护两份文案
const items: NavItem[] = [
  { id: 'dashboard', icon: 'home' },
  { id: 'system', icon: 'cpu' },
  { id: 'files', icon: 'folder' },
  { id: 'bridge', icon: 'swap' },
  { id: 'about', icon: 'info' },
]

const statusText = computed(() => t(nativeState.ok ? 'status.connected' : 'status.disconnected'))
</script>

<template>
  <nav class="sidebar">
    <div class="nav-group">
      <button
        v-for="it in items"
        :key="it.id"
        class="nav-item"
        :class="{ active: props.current === it.id }"
        :title="t(`nav.${it.id}.hint`)"
        @click="emit('change', it.id)"
      >
        <svg class="ic" viewBox="0 0 24 24" width="17" height="17" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <template v-if="it.icon === 'home'">
            <path d="M3.5 10.4 12 3.8l8.5 6.6V19a1.2 1.2 0 0 1-1.2 1.2h-4v-5.6h-6.6v5.6H4.7A1.2 1.2 0 0 1 3.5 19z" />
          </template>
          <template v-else-if="it.icon === 'cpu'">
            <rect x="7" y="7" width="10" height="10" rx="1.6" />
            <path d="M10 3.4v3M14 3.4v3M10 17.6v3M14 17.6v3M3.4 10h3M3.4 14h3M17.6 10h3M17.6 14h3" />
          </template>
          <template v-else-if="it.icon === 'folder'">
            <path d="M3.6 6.4A1.4 1.4 0 0 1 5 5h3.7l1.8 2H19a1.4 1.4 0 0 1 1.4 1.4v8.4A1.4 1.4 0 0 1 19 18.2H5a1.4 1.4 0 0 1-1.4-1.4z" />
          </template>
          <template v-else-if="it.icon === 'swap'">
            <path d="M4.5 8.5h13l-3-3M19.5 15.5h-13l3 3" />
          </template>
          <template v-else>
            <circle cx="12" cy="12" r="8.2" />
            <path d="M12 11v5.2M12 8.2v.1" />
          </template>
        </svg>
        <span class="label">{{ t(`nav.${it.id}`) }}</span>
      </button>
    </div>

    <div class="footer">
      <!-- 侧栏里的语种切换（标题栏之外的第二入口，方便窗口窄时使用） -->
      <button class="lang-row" :title="t('lang.switch')" @click="emit('toggle-locale')">
        <span class="lang-label">{{ t('lang.current') }}</span>
        <span class="lang-btn2">{{ LOCALE_LABEL[otherLocale] }}</span>
      </button>
      <div class="status" :class="{ ok: nativeState.ok }">
        <span class="dot" />
        <span>{{ statusText }}</span>
      </div>
      <div class="hint dim mono">{{ nativeState.transport }}</div>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  flex: 0 0 168px;
  display: flex;
  flex-direction: column;
  padding: 12px 10px;
  background: var(--panel-2);
  border-right: 1px solid var(--border);
}

.nav-group {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  height: 36px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-2);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  transition: background .13s, color .13s;
}

.nav-item:hover {
  background: #eceff4;
  color: var(--text);
}

.nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.ic {
  flex: 0 0 17px;
  opacity: .9;
}

.label {
  white-space: nowrap;
}

.footer {
  margin-top: auto;
  padding: 10px 8px 2px;
  border-top: 1px solid var(--border);
}

.lang-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  margin-bottom: 9px;
  padding: 5px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #fff;
  color: var(--text-2);
  font-size: 11.5px;
  transition: background .13s, border-color .13s;
}

.lang-row:hover {
  background: var(--accent-soft);
  border-color: #b9cdfb;
  color: var(--accent);
}

.lang-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.lang-btn2 {
  flex: 0 0 auto;
  padding: 1px 7px;
  border-radius: 20px;
  background: var(--accent);
  color: #fff;
  font-size: 10.5px;
  font-weight: 700;
}

.status {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--text-3);
}

.status.ok {
  color: var(--ok);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #c2c8d2;
}

.status.ok .dot {
  background: #2ba96b;
  box-shadow: 0 0 0 3px var(--ok-soft);
}

.hint {
  margin-top: 5px;
  font-size: 11px;
  word-break: break-all;
  line-height: 1.45;
}
</style>
