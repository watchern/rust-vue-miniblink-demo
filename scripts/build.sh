#!/usr/bin/env bash
# 一键构建：先打包 Vue 前端，再编译 Rust 宿主
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> [1/3] 检查 miniblink SDK"
if [ ! -f vendor/miniblink/mb132_x64.dll ]; then
  echo "    缺少 mb132_x64.dll，先下载 SDK..."
  bash scripts/fetch-miniblink.sh
else
  echo "    已存在 vendor/miniblink/mb132_x64.dll ✓"
fi

echo "==> [2/3] 构建前端 (Vue 3 + Vite)"
# 前端已在项目根（src/ + index.html + vite.config.ts），产物输出到 build_dist/
if [ ! -d node_modules ]; then
  npm install --registry=https://registry.npmmirror.com
fi
npm run build

echo "==> [3/3] 编译 Rust 宿主"
# 强制 target 落在项目内（本机环境变量 CARGO_TARGET_DIR 优先级高于配置文件）
export CARGO_TARGET_DIR="$ROOT/build_target"

if [ "${1:-}" = "--release" ]; then
  cargo build --release
  BIN="build_target/release/mbvue.exe"
else
  cargo build
  BIN="build_target/debug/mbvue.exe"
fi

echo
echo "构建完成: $BIN"
echo "运行: ./$BIN"
