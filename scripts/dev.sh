#!/usr/bin/env bash
# 开发模式：前端走 vite dev server（热更新），Rust 宿主连上去
#
#   ./scripts/dev.sh
#
# 会后台起 vite，然后启动 mbvue --dev。Ctrl-C 一起停掉。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ ! -f vendor/miniblink/mb132_x64.dll ]; then
  echo "缺少 miniblink SDK，先下载："
  bash scripts/fetch-miniblink.sh
fi

if [ ! -d node_modules ]; then
  echo "安装前端依赖..."
  npm install --registry=https://registry.npmmirror.com
fi

echo "启动 vite dev server (127.0.0.1:5173)..."
# 前端在项目根，无需再 cd web
npm run dev &
VITE_PID=$!

cleanup() {
  echo
  echo "关闭 vite ($VITE_PID)"
  kill "$VITE_PID" 2>/dev/null || true
}
trap cleanup EXIT

# 等 vite 起来
for i in $(seq 1 40); do
  if curl -fsS -m 2 http://127.0.0.1:5173/ >/dev/null 2>&1; then
    echo "vite 就绪 ✓"
    break
  fi
  sleep 0.5
done

echo "启动 Rust 宿主..."
export CARGO_TARGET_DIR="$ROOT/build_target"
cargo build 2>&1 | tail -5
./build_target/debug/mbvue.exe --dev
