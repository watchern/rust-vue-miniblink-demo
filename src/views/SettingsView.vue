<script setup lang="ts">
import { t, locale, setLocale, theme, setTheme } from '../i18n'
import type { Locale, Theme } from '../i18n'

const localeOptions: { value: Locale; labelKey: string }[] = [
  { value: 'zh-CN', labelKey: 'settings.lang.zh' },
  { value: 'en-US', labelKey: 'settings.lang.en' },
]

const themeOptions: { value: Theme; labelKey: string }[] = [
  { value: 'light', labelKey: 'settings.theme.light' },
  { value: 'dark', labelKey: 'settings.theme.dark' },
  { value: 'system', labelKey: 'settings.theme.system' },
]
</script>

<template>
  <div class="view">
    <h1 class="view-title">{{ t('settings.title') }}</h1>
    <p class="view-sub">{{ t('settings.sub') }}</p>

    <div class="section-title">{{ t('settings.lang') }}</div>
    <div class="card pad">
      <p class="hint">{{ t('settings.langHint') }}</p>
      <div class="seg">
        <button
          v-for="o in localeOptions"
          :key="o.value"
          class="seg-item"
          :class="{ active: locale === o.value }"
          :aria-pressed="locale === o.value"
          @click="setLocale(o.value)"
        >
          {{ t(o.labelKey) }}
        </button>
      </div>
    </div>

    <div class="section-title">{{ t('settings.theme') }}</div>
    <div class="card pad">
      <p class="hint">{{ t('settings.themeHint') }}</p>
      <div class="seg">
        <button
          v-for="o in themeOptions"
          :key="o.value"
          class="seg-item"
          :class="{ active: theme === o.value }"
          :aria-pressed="theme === o.value"
          @click="setTheme(o.value)"
        >
          {{ t(o.labelKey) }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hint {
  margin: 0 0 12px;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--text-2);
}

.seg {
  display: inline-flex;
  gap: 6px;
  flex-wrap: wrap;
}

.seg-item {
  height: 32px;
  padding: 0 16px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--panel);
  color: var(--text);
  font-size: 13px;
  font-weight: 500;
  transition: background .14s, border-color .14s, color .14s, transform .06s;
}

.seg-item:hover {
  background: var(--hover-bg);
}

.seg-item:active {
  transform: translateY(1px);
}

.seg-item.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.seg-item.active:hover {
  background: #2f5fe0;
}
</style>
