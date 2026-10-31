#!/usr/bin/env bash
# 一键构建：先打包 Vue 前端，再编译 Rust 宿主（Windows / Linux 通用）
#
#   Windows：需要 miniblink SDK（vendor/miniblink/mb132_x64.dll）
#   Linux：  无需 miniblink（WebKitGTK 壳），但要有
#            libgtk-3-dev libwebkit2gtk-4.1-dev
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OS="$(uname -s)"
EXE="mbvue"
if [[ "$OS" =~ ^MINGW|^MSYS|^CYGWIN ]]; then
  EXE="mbvue.exe"
fi

if [[ "$EXE" == "mbvue.exe" ]]; then
  echo "==> [1/3] 检查 miniblink SDK"
  if [ ! -f vendor/miniblink/mb132_x64.dll ]; then
    echo "    缺少 mb132_x64.dll，先下载 SDK..."
    bash scripts/fetch-miniblink.sh
  else
    echo "    已存在 vendor/miniblink/mb132_x64.dll ✓"
  fi
else
  echo "==> [1/3] Linux 环境（WebKitGTK 壳，无需 miniblink）"
  if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    echo "    缺少 webkit2gtk-4.1 开发文件，Ubuntu/Debian 安装：" >&2
    echo "      sudo apt install -y libgtk-3-dev libwebkit2gtk-4.1-dev" >&2
    exit 1
  fi
  echo "    webkit2gtk-4.1 ✓ ($(pkg-config --modversion webkit2gtk-4.1))"
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
  BIN="build_target/release/$EXE"
else
  cargo build
  BIN="build_target/debug/$EXE"
fi

echo
echo "构建完成: $BIN"
echo "运行: ./$BIN"
