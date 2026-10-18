<script setup lang="ts">
import { t } from '../i18n'

const layers = [
  { name: 'Vue 3 + Vite', roleKey: 'about.layer.ui', descKey: 'about.layer.uiDesc', color: '#3b6ef5' },
  { name: 'miniblink mb132', roleKey: 'about.layer.render', descKey: 'about.layer.renderDesc', color: '#46a13a' },
  { name: 'Rust', roleKey: 'about.layer.native', descKey: 'about.layer.nativeDesc', color: '#e07b39' },
]

// [命令, 说明的 i18n key]
const commands: [string, string][] = [
  ['app.info', 'dash.section.appInfo'],
  ['sys.info / sys.memory', 'sys.section.host'],
  ['fs.drives / fs.list / fs.read / fs.write', 'files.title'],
  ['win.minimize / win.maximize / win.close / win.drag / win.title', 'about.cmd.win'],
  ['echo / compute.fib / compute.hash', 'bridge.sec.calc'],
  ['event.start / event.stop', 'bridge.sec.push'],
]

const trimmed = [
  ['mb132_x32.dll', '55 MB'],
  ['mbvue.pdb', '1.8 MB'],
  ['build_target/ deps/ incremental/', 'cargo 中间产物'],
  ['build_dist/', '已编译期内嵌进 exe'],
  ['src/ src-rs/ vendor/', '源码与 SDK 不随包分发'],
]

const tree = `rust-vue-miniblink-demo/
├─ src-rs/          Rust 宿主源码（原 src/）
│  ├─ main.rs      入口：加载 DLL、建窗口、消息循环
│  ├─ ffi.rs       miniblink C API 的 Rust 绑定（动态解析符号）
│  ├─ i18n.rs      壳侧文案表（IPC 报错随语种切换）
│  ├─ ipc.rs       命令分发 + Rust→JS 事件推送
│  ├─ server.rs    内置 HTTP 服务：release 喂内嵌资源表，开发期喂 build_dist/
│  └─ build.rs→    生成前端资源表（include_bytes!）并部署 DLL/图标
├─ src/            Vue 3 前端（index.html + vite.config.ts 在根）
│  ├─ i18n/        中英文案表（零依赖，两种语种）
│  └─ bridge.ts    mbQuery → Promise 的桥接层
├─ assets/         icon.ico / icon.rc（exe 内嵌 + 窗口图标）
├─ build_dist/     前端产物（vite build 输出；构建时被内嵌进 exe）
├─ build_target/   cargo 产物 + install/ 便携目录 + mbvue-Setup.exe
├─ vendor/miniblink/  mb132_x64.dll + mb.h（68MB，装包时改名 core.dll）
└─ scripts/        构建、打包与 SDK 获取脚本`
</script>

<template>
  <div class="view">
    <h1 class="view-title">{{ t('about.title') }}</h1>
    <p class="view-sub">{{ t('about.sub') }}</p>

    <div class="section-title">{{ t('about.section.layers') }}</div>
    <div class="layers">
      <div v-for="l in layers" :key="l.name" class="card pad layer">
        <div class="dot" :style="{ background: l.color }" />
        <div>
          <div class="l-name">{{ l.name }} <span class="dim">{{ t(l.roleKey) }}</span></div>
          <div class="l-desc">{{ t(l.descKey) }}</div>
        </div>
      </div>
    </div>

    <div class="section-title">{{ t('about.section.flow') }}</div>
    <div class="card pad">
      <div class="flow">
        <span class="node">Vue</span>
        <span class="arrow">── mbQuery(JSON) ──▶</span>
        <span class="node">mbOnJsQuery</span>
        <span class="arrow">──▶</span>
        <span class="node rust">Rust 命令表</span>
      </div>
      <div class="flow">
        <span class="node rust">Rust 线程</span>
        <span class="arrow">── mbRunJs ──▶</span>
        <span class="node">window.__mbEmit</span>
        <span class="arrow">──▶</span>
        <span class="node">Vue 订阅者</span>
      </div>
      <p class="note dim">{{ t('about.flow.note') }}</p>
    </div>

    <div class="section-title">{{ t('about.section.cmds') }}</div>
    <div class="card pad">
      <div v-for="[c, d] in commands" :key="c" class="cmd">
        <span class="mono cmd-name">{{ c }}</span>
        <span class="dim">{{ t(d) }}</span>
      </div>
    </div>

    <div class="section-title">{{ t('about.section.installer') }}</div>
    <div class="card pad">
      <p class="note">
        {{ t('about.installer.desc', {
          file: 'mbvue-Setup.exe',
          files: 'mbvue.exe / core.dll / icon.ico',
          size: '≈ 66.6 MB',
        }) }}
      </p>
      <div class="trimmed">{{ t('about.installer.trimmed') }}</div>
      <div v-for="[f, why] in trimmed" :key="f" class="cmd">
        <span class="mono cmd-name">{{ f }}</span>
        <span class="dim">{{ why }}</span>
      </div>
    </div>

    <div class="section-title">{{ t('about.section.tree') }}</div>
    <pre class="tree mono">{{ tree }}</pre>

    <div class="section-title">{{ t('about.section.ops') }}</div>
    <div class="card pad">
      <div class="kv"><span class="k">{{ t('about.op.dev') }}</span><span class="v">pnpm run dev 后 ./build_target/debug/mbvue.exe --dev</span></div>
      <div class="kv"><span class="k">{{ t('about.op.build') }}</span><span class="v">scripts/build.sh</span></div>
      <div class="kv"><span class="k">{{ t('about.op.dist') }}</span><span class="v">scripts/make_installer.sh</span></div>
      <div class="kv"><span class="k">{{ t('about.op.refetch') }}</span><span class="v">scripts/fetch-miniblink.sh</span></div>
    </div>
  </div>
</template>

<style scoped>
.layers {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.layer {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  margin-top: 5px;
  flex: 0 0 9px;
}

.l-name {
  font-size: 13.5px;
  font-weight: 650;
}

.l-desc {
  font-size: 12.5px;
  color: var(--text-2);
  margin-top: 3px;
  line-height: 1.6;
}

.flow {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 7px 0;
  font-size: 12.5px;
}

.node {
  padding: 3px 9px;
  border-radius: 5px;
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
  font-size: 12px;
}

.node.rust {
  background: #fdf3e2;
  color: #a86400;
}

.arrow {
  color: var(--text-3);
  font-size: 11.5px;
}

.note {
  margin: 8px 0 0;
  font-size: 12.5px;
  line-height: 1.7;
}

.trimmed {
  margin: 12px 0 6px;
  font-size: 12.5px;
  font-weight: 600;
}

.cmd {
  display: flex;
  gap: 12px;
  align-items: baseline;
  padding: 5px 0;
  border-bottom: 1px dashed var(--border);
  font-size: 12.5px;
  flex-wrap: wrap;
}

.cmd:last-child {
  border-bottom: 0;
}

.cmd-name {
  color: var(--text);
  font-weight: 600;
  min-width: 260px;
}

.tree {
  margin: 0;
  padding: 14px 16px;
  background: #1e2430;
  color: #d7dce5;
  border-radius: var(--radius-sm);
  overflow-x: auto;
  line-height: 1.7;
  user-select: text;
}
</style>
