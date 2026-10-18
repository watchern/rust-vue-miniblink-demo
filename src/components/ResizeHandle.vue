<script setup lang="ts">
import { onUnmounted } from 'vue'
import { bridge } from '../bridge'

// ---------------------------------------------------------------------------
// 无边框窗口的 resize 把手（右缘 / 下缘 / 右下角）。
//
// 实测机制（2026-10 于本机 1366x768 / DPI 100% 环境，SetWinEventHook 间谍 +
// 逐轮询日志三方对照验证，结论同时适用于 Node/koffi 与 Rust 两套宿主）：
//
//   第一层｜系统原生 resize 循环（窗口外 0~4px，即 WS_THICKFRAME 边框区）
//     WS_THICKFRAME 已保留；WM_NCHITTEST 返回 HTRIGHT/HTBOTTOM 等后，
//     miniblink 的 WndProc 把非 HTCAPTION 的 WM_NCLBUTTONDOWN 透传给
//     DefWindowProc → 原生 resize 模态循环启动（间谍实测
//     EVENT_SYSTEM_MOVESIZESTART/END，窗口尺寸精确跟随鼠标）。
//     边界：仅在窗口处于前台/激活态时生效；非激活态该区域 mousedown 被
//     系统用于激活窗口，循环不启动。
//
//   第二层｜本组件的前端 handle（客户区外 6px ≈ 窗口内 4~10px）
//     webview 收到 mousedown → win.getSize 读基准 → mousemove 按增量算目标
//     尺寸 → win.resize 下发 SetWindowPos(SWP_NOMOVE)（左上角不动）。
//     与前台状态无关，恒可工作，和第一层互补覆盖整个 ~10px 缩放带。
//
//   第三层｜最小尺寸钳制：Node 版 TIMERPROC 300ms 轮询守卫，Rust 版
//     WM_GETMINMAXINFO 原生子类化；前端再钳一次做双保险。
//
// 历史误判警示：早前「miniblink 不走系统 resize 路径、合成拖拽无效」的
// 结论是在外部驱动干扰环境下测得的（当时有后台脚本在同时操作窗口），
// 已作废——干净环境下两层均正常，本组件不是死代码，勿删。
// ---------------------------------------------------------------------------

const MIN_W = 900
const MIN_H = 600
const THROTTLE_MS = 30 // 拖拽中下发节流，30ms ≈ 33fps 足够顺滑且不刷爆 IPC

type Dir = 'e' | 's' | 'se'

let baseW = 0
let baseH = 0
let startX = 0
let startY = 0
let lastSend = 0
let dir: Dir = 'e'

function onMove(e: MouseEvent) {
  const now = performance.now()
  if (now - lastSend < THROTTLE_MS) return
  lastSend = now
  const dx = e.clientX - startX
  const dy = e.clientY - startY
  const w = Math.max(MIN_W, baseW + (dir === 'e' || dir === 'se' ? dx : 0))
  const h = Math.max(MIN_H, baseH + (dir === 's' || dir === 'se' ? dy : 0))
  bridge.windowResize(w, h).catch(() => {})
}

function endDrag() {
  window.removeEventListener('mousemove', onMove)
  window.removeEventListener('mouseup', endDrag)
  document.body.style.userSelect = ''
  document.body.style.cursor = ''
}

async function startDrag(e: MouseEvent, d: Dir) {
  if (e.button !== 0) return
  e.preventDefault() // 阻止拖拽时选中文本 / 触发原生行为
  // 基准尺寸以 mousedown 时刻为准（而非写死的设计尺寸），最大化后往下拖也能正确还原
  const size = await bridge.windowSize().catch(() => null)
  if (!size || size.w <= 0 || size.h <= 0) return
  dir = d
  baseW = size.w
  baseH = size.h
  startX = e.clientX
  startY = e.clientY
  lastSend = 0
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', endDrag)
  document.body.style.userSelect = 'none'
  document.body.style.cursor = d === 'e' ? 'ew-resize' : d === 's' ? 'ns-resize' : 'nwse-resize'
}

onUnmounted(endDrag)
</script>

<template>
  <!-- 右缘：从标题栏下方到底边（避开右下角把手） -->
  <div class="rz rz-e" title="横向调整" @mousedown="startDrag($event, 'e')" />
  <!-- 下缘：从左缘到右缘（避开右下角把手） -->
  <div class="rz rz-s" title="纵向调整" @mousedown="startDrag($event, 's')" />
  <!-- 右下角把手 -->
  <div class="rz rz-se" title="斜向调整" @mousedown="startDrag($event, 'se')">
    <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
      <path d="M9.2 1.2 1.2 9.2M9.2 5.2 5.2 9.2" stroke="currentColor" stroke-width="1.1"
        stroke-linecap="round" fill="none" />
    </svg>
  </div>
</template>

<style scoped>
.rz {
  position: absolute;
  z-index: 90;
  pointer-events: auto;
  touch-action: none;
}

.rz-e {
  top: 46px; /* 标题栏高度，让标题栏拖拽区保持完整 */
  right: 0;
  bottom: 16px;
  width: 6px;
  cursor: ew-resize;
}

.rz-s {
  left: 16px;
  right: 16px;
  bottom: 0;
  height: 6px;
  cursor: ns-resize;
}

.rz-se {
  right: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  display: grid;
  place-items: center;
  cursor: nwse-resize;
  color: var(--text-3);
}

/*  hover 反馈：边缘微亮 + 角把手显现（与 Win11 一致：平时隐入背景，悬停才显示） */
.rz-e:hover,
.rz-s:hover {
  background: rgba(59, 110, 245, 0.14);
}

.rz-se {
  opacity: 0;
  transition: opacity 0.15s;
}

.rz-se:hover {
  opacity: 1;
}
</style>
