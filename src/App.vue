<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import TitleBar from './components/TitleBar.vue'
import SideBar from './components/SideBar.vue'
import ResizeHandle from './components/ResizeHandle.vue'
import DashboardView from './views/DashboardView.vue'
import SystemView from './views/SystemView.vue'
import FilesView from './views/FilesView.vue'
import BridgeView from './views/BridgeView.vue'
import AboutView from './views/AboutView.vue'
import SettingsView from './views/SettingsView.vue'
import { t, locale, toggleLocale, loadPersistedConfig } from './i18n'
import { bridge } from './bridge'

const views = {
  dashboard: DashboardView,
  system: SystemView,
  files: FilesView,
  bridge: BridgeView,
  settings: SettingsView,
  about: AboutView,
} as const

type ViewId = keyof typeof views

const current = ref<ViewId>('dashboard')
const view = computed(() => views[current.value])

const subtitle = computed(() => t(`sub.${current.value}`))

/**
 * 语种切换后做两件联动：
 *   ① 窗口标题同步给 Rust —— 任务栏按钮 / Alt+Tab 预览 / 窗口标题栏都读 OS 窗口标题，
 *      只改前端不会反映到那里；
 *   ② 语种同步给壳（app.setLocale）—— 这样 IPC 报错（如「读取目录失败」）也会
 *      跟着界面语言变，否则英文界面下弹出中文报错会很突兀。
 */
watch(
  locale,
  (l) => {
    void bridge.setTitle(t('app.title')).catch(() => {})
    void bridge.setLocale(l).catch(() => {})
    // 顺带让「文档标题」也一致（浏览器调试时看标签页）
    if (typeof document !== 'undefined') document.title = `mbvue · ${l}`
  },
  { immediate: true },
)

function onToggleLocale() {
  toggleLocale()
}

// 启动后从 config.json 读回上次保存的语种 / 主题（解决"重开回到默认中文"）。
// 放在 onMounted：读盘走 Rust IPC，页面挂载、桥接就绪后再读最稳。
onMounted(() => {
  void loadPersistedConfig().catch(() => {})
})
</script>

<template>
  <div class="app">
    <TitleBar
      :title="t('app.title')"
      :subtitle="subtitle"
      @toggle-locale="onToggleLocale"
    />
    <div class="body">
      <SideBar
        :current="current"
        @change="(id) => (current = id as ViewId)"
        @toggle-locale="onToggleLocale"
      />
      <main class="main">
        <KeepAlive>
          <component :is="view" :key="current" />
        </KeepAlive>
      </main>
    </div>
    <ResizeHandle />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative; /* resize 把手绝对定位的锚点 */
  background: var(--bg);
}

.body {
  flex: 1;
  display: flex;
  min-height: 0;
}

.main {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}
</style>
