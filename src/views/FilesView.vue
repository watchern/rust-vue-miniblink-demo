<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { bridge, type FsList } from '../bridge'
import { fmtBytes, fmtTime } from '../format'
import { t } from '../i18n'

const pathInput = ref('.')
const list = ref<FsList | null>(null)
const drives = ref<{ name: string; path: string }[]>([])
const loading = ref(false)
const error = ref('')

const preview = ref<{ path: string; size: number; truncated: boolean; content: string } | null>(null)
const draft = ref('')
const saving = ref(false)
const toast = ref('')

async function open(p: string) {
  loading.value = true
  error.value = ''
  try {
    list.value = await bridge.listDir(p)
    pathInput.value = list.value.path
    preview.value = null
    draft.value = ''
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function readFile(p: string) {
  error.value = ''
  try {
    preview.value = await bridge.readFile(p)
    draft.value = preview.value.content
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function save() {
  if (!preview.value) return
  saving.value = true
  error.value = ''
  try {
    const r = await bridge.writeFile(preview.value.path, draft.value)
    toast.value = t('files.toast.written', { n: r.bytes })
    preview.value.content = draft.value
    setTimeout(() => (toast.value = ''), 2600)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}

function goUp() {
  if (list.value?.parent) open(list.value.parent)
}

async function writeSample() {
  const p = (list.value?.path || '.') + '/miniblink-demo.txt'
  try {
    await bridge.writeFile(p, t('files.sampleContent', { time: new Date().toLocaleString() }))
    toast.value = t('files.toast.created', { p })
    setTimeout(() => (toast.value = ''), 2600)
    if (list.value) open(list.value.path)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

onMounted(async () => {
  try {
    drives.value = await bridge.drives()
  } catch {
    /* 忽略 */
  }
  open('.')
})
</script>

<template>
  <div class="view">
    <h1 class="view-title">{{ t('files.title') }}</h1>
    <p class="view-sub">{{ t('files.sub', { cmds: 'fs.*' }) }}</p>

    <div class="toolbar">
      <button class="btn sm" :disabled="!list?.parent" @click="goUp">{{ t('files.btn.up') }}</button>
      <button class="btn sm" :disabled="loading" @click="open(pathInput)">{{ t('common.refresh') }}</button>
      <input
        v-model="pathInput"
        class="input path-input mono"
        :placeholder="t('files.ph.path')"
        @keydown.enter="open(pathInput)"
      />
      <div class="drives">
        <button
          v-for="d in drives"
          :key="d.path"
          class="btn sm"
          :title="d.path"
          @click="open(d.path)"
        >
          {{ d.name }}
        </button>
      </div>
      <button class="btn sm" @click="writeSample">{{ t('files.btn.writeSample') }}</button>
    </div>

    <div v-if="error" class="err-line"><strong>{{ t('common.errorPrefixShort') }}</strong>{{ error }}</div>

    <div class="panes">
      <!-- 列表 -->
      <div class="card list-pane">
        <div class="pane-head">
          <span class="mono dim">{{ list?.path ?? '' }}</span>
          <span v-if="loading" class="dim">{{ t('common.loading') }}</span>
        </div>
        <div class="list">
          <template v-if="list">
            <div
              v-for="d in list.dirs"
              :key="d.path"
              class="row dir"
              @dblclick="open(d.path)"
              @click="open(d.path)"
            >
              <span class="icon">📁</span>
              <span class="name">{{ d.name }}</span>
              <span class="meta dim">{{ fmtTime(d.modified) }}</span>
            </div>
            <div
              v-for="f in list.files"
              :key="f.path"
              class="row"
              :class="{ active: preview?.path === f.path }"
              @click="readFile(f.path)"
            >
              <span class="icon">📄</span>
              <span class="name">{{ f.name }}</span>
              <span class="meta dim">{{ fmtBytes(f.size) }}</span>
            </div>
            <div v-if="!list.dirs.length && !list.files.length" class="empty">{{ t('files.emptyDir') }}</div>
          </template>
          <div v-else class="empty">{{ t('common.loading') }}</div>
        </div>
      </div>

      <!-- 预览 -->
      <div class="card preview-pane">
        <div class="pane-head">
          <span class="mono dim">{{ preview?.path ?? t('files.noFile') }}</span>
          <span v-if="preview" class="dim">{{ fmtBytes(preview.size) }}</span>
        </div>
        <div class="preview-body">
          <textarea
            v-if="preview"
            v-model="draft"
            class="editor mono"
            spellcheck="false"
          />
          <div v-else class="empty">{{ t('files.clickHint') }}</div>
        </div>
        <div class="preview-foot">
          <span v-if="preview?.truncated" class="tag warn">{{ t('files.truncated') }}</span>
          <span v-if="toast" class="tag ok">{{ toast }}</span>
          <span class="spacer" />
          <button class="btn sm primary" :disabled="!preview || saving" @click="save">
            {{ saving ? t('files.saving') : t('files.save') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}

.path-input {
  flex: 1;
  min-width: 180px;
  user-select: text;
}

.drives {
  display: flex;
  gap: 4px;
}

.panes {
  display: grid;
  grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
  gap: 14px;
  height: calc(100% - 130px);
  min-height: 340px;
}

.list-pane,
.preview-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.pane-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
  flex: 0 0 auto;
}

.list {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}

.row {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 6px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  cursor: default;
}

.row:hover {
  background: var(--panel-2);
}

.row.active {
  background: var(--accent-soft);
  color: var(--accent);
}

.row.dir .name {
  font-weight: 600;
}

.icon {
  flex: 0 0 auto;
  width: 16px;
  text-align: center;
  opacity: .85;
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta {
  flex: 0 0 auto;
  font-size: 11.5px;
}

.preview-body {
  flex: 1;
  min-height: 0;
  padding: 10px 12px;
}

.editor {
  width: 100%;
  height: 100%;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 10px 12px;
  background: var(--panel-2);
  resize: none;
  outline: none;
  line-height: 1.6;
  user-select: text;
}

.editor:focus {
  border-color: var(--accent);
  background: #fff;
}

.preview-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
  flex: 0 0 auto;
}

.spacer {
  flex: 1;
}

.err-line {
  padding: 8px 12px;
  margin-bottom: 10px;
  border-radius: var(--radius-sm);
  background: var(--err-soft);
  color: var(--err);
  font-size: 12.5px;
}
</style>
