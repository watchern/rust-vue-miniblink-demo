#!/usr/bin/env bash
# 打安装包：先准备便携目录 install/，再用 NSIS 压成单文件 Setup.exe。
#
# 为什么要分两步：
#   install/  是「装完之后的目录」，可以单独拷走直接跑（绿色版），也能验证清单；
#   Setup.exe 是给终端用户双击的安装器。
# 两份产物共用同一份文件清单，所以不会出现"安装后少文件"的问题。
#
# 用法：
#   scripts/make_installer.sh            # 完整：cargo build --release + 前端 + 打包
#   scripts/make_installer.sh --no-build # 跳过编译，只用现有产物打包
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# 强制 cargo 产物落在项目内：本机会话注入 CARGO_TARGET_DIR=H:\trae-build-target，
# 而环境变量优先级高于 .cargo/config.toml 的 [build] target-dir，
# 不覆盖的话会编译到项目外、而下面的 TARGET 路径仍是项目内 → 打进安装包的是旧 exe。
# （scripts/build.sh 已有同样处理，两边保持一致）
export CARGO_TARGET_DIR="$ROOT/build_target"

TARGET="build_target/release"
DIST="build_dist"
INSTALL_DIR="build_target/install"
OUT="build_target/mbvue-Setup.exe"

DO_BUILD=1
[[ "${1:-}" == "--no-build" ]] && DO_BUILD=0

# ---------------------------------------------------------------- 找 makensis
# 本机没装独立 NSIS，但 electron-builder 的缓存里有一份完整的 NSIS 3.0.4.1，
# 直接借用来编译，省一次下载。
find_makensis() {
  if command -v makensis >/dev/null 2>&1; then
    command -v makensis
    return 0
  fi
  local roots=(
    "$LOCALAPPDATA/electron-builder/Cache/nsis"
    "$APPDATA/electron-builder/Cache/nsis"
  )
  for r in "${roots[@]}"; do
    [[ -d "$r" ]] || continue
    # 缓存结构：<root>/nsis-3.0.4.1/[hash]/... 或 <root>/nsis-3.0.4.1/Bin/makensis.exe
    local exe
    exe="$(find "$r" -iname 'makensis.exe' -print -quit 2>/dev/null || true)"
    if [[ -n "$exe" ]]; then
      echo "$exe"
      return 0
    fi
  done
  # choco / 独立安装器的标准位置（choco 修改的是注册表 PATH，当前进程看不到）
  for p in "/c/Program Files (x86)/NSIS/makensis.exe" "/c/Program Files/NSIS/makensis.exe"; do
    [[ -f "$p" ]] && { echo "$p"; return 0; }
  done
  return 1
}

if ! MAKENSIS="$(find_makensis)"; then
  echo "[installer] 找不到 makensis。" >&2
  echo "  方案一：装独立 NSIS（https://nsis.sourceforge.io/），装完直接重跑本脚本。" >&2
  echo "  方案二：跑一次 npx electron-builder 让它把 NSIS 缓存下来（产物会被复用）。" >&2
  exit 1
fi
echo "[installer] makensis: $MAKENSIS"

# ---------------------------------------------------------------- 构建
if [[ $DO_BUILD == 1 ]]; then
  echo "[installer] 1/4 构建前端…"
  # CI=true 是必要的：pnpm 11 在跑 build 前会做一次 deps 状态检查，
  # 发现 node_modules 与 lock 不一致时想重建 modules 目录，
  # 而非交互 shell（无 TTY）下它会直接 Aborted —— ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY。
  # 这类"顺手清理"对本脚本（只调 vite build）没必要，跳过即可。
  CI=true pnpm run build

  echo "[installer] 2/4 构建 Rust（release）…"
  cargo build --release
fi

# 校验产物新鲜度：exe 若比 dist/index.html 旧，说明 --no-build 用的是陈旧产物
if [[ -f "$TARGET/mbvue.exe" && -f "$DIST/index.html" ]]; then
  if [[ "$TARGET/mbvue.exe" -ot "$DIST/index.html" ]]; then
    echo "[installer] 警告：mbvue.exe 比前端产物还旧，打包进去的可能是上一版。" >&2
    echo "[installer]       不带 --no-build 重跑一次即可。" >&2
  fi
fi

[[ -f "$TARGET/mbvue.exe" ]]   || { echo "缺少 $TARGET/mbvue.exe" >&2; exit 1; }
[[ -f "$TARGET/mb132_x64.dll" ]] || { echo "缺少 $TARGET/mb132_x64.dll" >&2; exit 1; }
# 前端产物不进安装包，但**必须存在**：build.rs 要在编译期把它内嵌进 exe。
# 少了它不会编译失败（内嵌表为空、运行时报 404），所以这里显式拦一道。
[[ -f "$DIST/index.html" ]]    || { echo "缺少 $DIST/index.html（先跑 pnpm run build，否则 exe 里没有前端）" >&2; exit 1; }

# ---------------------------------------------------------------- 组装便携目录
echo "[installer] 3/4 组装便携目录…"

# 沙箱对批量删除有保护（≥50 文件会拦），这里只删自己建的少量文件，用 rm -rf 风险可控；
# 但为保险起见先判断目录里文件数，避免误删用户数据。
if [[ -d "$INSTALL_DIR" ]]; then
  n=$(find "$INSTALL_DIR" -type f | wc -l)
  if (( n > 50 )); then
    echo "警告：$INSTALL_DIR 里有 $n 个文件，超过 50，可能是别的东西，跳过清理。" >&2
  else
    rm -rf "$INSTALL_DIR"
  fi
fi
mkdir -p "$INSTALL_DIR"

# 精简清单（只 3 项文件，共 ~66.6MB）：
#   mbvue.exe   主程序（exe 内嵌 Explorer 图标 + **前端产物**）
#   core.dll    miniblink 内核（安装包里从 mb132_x64.dll 改名而来）
#   icon.ico运行时窗口图标（任务栏/Alt+Tab）
# 关键变化（2026-10-18）：前端产物已在 **编译期内嵌进 exe**（build.rs 生成
# 资源表 + include_bytes!），所以安装目录**不再需要 build_dist/**。
# 明确排除：mb132_x32.dll(55MB，x64 进程无法加载)、*.pdb、deps/、incremental/、
#           src/、src-rs/、vendor/ 等源码与 cargo 中间产物。
cp "$TARGET/mbvue.exe"       "$INSTALL_DIR/"
# 改名：vendor 里叫 mb132_x64.dll，安装包里叫 core.dll（更贴合"内核"语义）。
# 两边名不同没关系——main.rs 的 find_dll() 两个名字都认。
cp "$TARGET/mb132_x64.dll"   "$INSTALL_DIR/core.dll"
cp assets/icon.ico           "$INSTALL_DIR/"

# 本机 Git Bash 没有 bc，用 awk 做浮点（只用于打印体积，可读性优先，精度足够）
mb() { awk -v b="${1:-0}" 'BEGIN{printf "%.2f", b/1048576}'; }

echo "[installer] 便携目录内容："
( cd "$INSTALL_DIR" && find . -type f | sed 's|^\./||' | sort ) | while read -r f; do
  sz=$(stat -c%s "$INSTALL_DIR/$f")
  printf '    %-46s %9s MB\n' "$f" "$(mb "$sz")"
done

PORTABLE_BYTES=$(du -sb "$INSTALL_DIR" | cut -f1)
echo "[installer] 便携目录合计: $(mb "$PORTABLE_BYTES") MB"

# ---------------------------------------------------------------- 打 Setup.exe
echo "[installer] 4/4 编译 NSIS 安装包…"
mkdir -p build_target

# makensis 对源文件编码很挑：UTF-8 无 BOM 会报 "Bad text encoding"。
# 这里统一转成带 BOM 的 UTF-8（NSIS 3.x 原生支持 UTF-8 with BOM，且能正确处理中文字符串）。
# 临时文件放项目内的 build_target/，不用系统 TEMP（沙箱会校验 mktemp 出来的绝对盘符路径）。
NSI_TMP="$ROOT/build_target/.installer-bom.nsi"
mkdir -p build_target
# 用 node 写 BOM（pnpm 环境自带 node，避免依赖 iconv/perl）
node -e '
  const fs = require("fs");
  const src = fs.readFileSync(process.argv[1]);
  fs.writeFileSync(process.argv[2], Buffer.concat([Buffer.from([0xEF,0xBB,0xBF]), src]));
' scripts/make_installer.nsi "$NSI_TMP"
trap 'rm -f "$NSI_TMP"' EXIT

"$MAKENSIS" \
  -DSRC_RELEASE="$ROOT/$TARGET" \
  -DSRC_DIST="$ROOT/$DIST" \
  -DSRC_ASSETS="$ROOT/assets" \
  -DOUT_DIR="$ROOT/build_target" \
  -V2 \
  "$NSI_TMP"

SETUP_BYTES=$(stat -c%s "$OUT")
echo ""
echo "============================================================"
echo "[installer] 便携目录 : $INSTALL_DIR"
echo "[installer] 安装包   : $OUT"
echo "[installer] 安装包大小: $(mb "$SETUP_BYTES") MB"
echo "[installer] 压缩率   : $(awk -v s="$SETUP_BYTES" -v p="$PORTABLE_BYTES" 'BEGIN{printf "%.1f", s*100/p}')% of portable"
echo "============================================================"
