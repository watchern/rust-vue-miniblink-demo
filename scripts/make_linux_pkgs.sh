#!/usr/bin/env bash
# Linux 打包：deb / rpm / AppImage / tar.gz 便携包。
#
# 与 Windows 侧 make_installer.sh 的分工：
#   Windows: NSIS Setup.exe（安装包）+ install/ 便携目录（可再压 zip）
#   Linux:   cargo-deb → .deb   （Debian/Ubuntu 安装包，depends 由 shlibs 自动算）
#            cargo-generate-rpm → .rpm（Fedora/openSUSE 安装包）
#            linuxdeploy → .AppImage（自包含运行时，含 WebKitGTK 多进程助手）
#            tar.gz 便携包（解压即用，兜底格式，包管理器坏了也能跑）
#
# 外部工具：
#   cargo-deb / cargo-generate-rpm —— CI 用 taiki-e/install-action 装，
#     本地 cargo install <name>（deb 的 depends=$auto 需要 ldd 可用）
#   linuxdeploy-x86_64.AppImage + AppRun —— 脚本自动下载缓存到 build_target/tools/，
#     可用 LINUXDEPLOY_URL / APPRUN_URL 覆盖（GitHub 直连不通时走镜像前缀）
#
# 产物（build_target/dist/）：
#   mbvue-<ver>-linux-x64.{deb,rpm,AppImage,tar.gz}
#
# 用法：
#   scripts/make_linux_pkgs.sh            # 完整：前端 + cargo build --release + 打包
#   scripts/make_linux_pkgs.sh --no-build # 跳过编译，只用现有产物打包
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# 与 make_installer.sh 同理：强制 cargo 产物落在项目内，
# 防止外部 CARGO_TARGET_DIR 把 exe 指到别处导致打包进旧产物
export CARGO_TARGET_DIR="$ROOT/build_target"

TARGET="build_target/release"
DIST="build_target/dist"
TOOLS="build_target/tools"

ARCH="$(uname -m)"
if [[ "$ARCH" != "x86_64" ]]; then
  echo "[linux-pkg] 仅支持 x86_64（当前 $ARCH；产物名/助手路径都写死了 x64）" >&2
  exit 1
fi

VER="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME_BASE="mbvue-${VER}-linux-x64"

DO_BUILD=1
[[ "${1:-}" == "--no-build" ]] && DO_BUILD=0

if [[ $DO_BUILD == 1 ]]; then
  echo "[linux-pkg] 1/5 构建前端…"
  # CI=true：非交互 shell 下 pnpm 想重建 modules 目录会直接 Aborted，跳过即可
  CI=true pnpm run build
  echo "[linux-pkg] 2/5 编译 Rust（release）…"
  cargo build --release
else
  echo "[linux-pkg] 跳过构建（--no-build）"
fi

[[ -f "$TARGET/mbvue" ]]          || { echo "缺少 $TARGET/mbvue" >&2; exit 1; }
[[ -f "build_dist/index.html" ]]  || { echo "缺少 build_dist/index.html（先 pnpm run build，否则 exe 里没有前端）" >&2; exit 1; }
mkdir -p "$DIST" "$TOOLS"

mb() { awk -v b="${1:-0}" 'BEGIN{printf "%.2f", b/1048576}'; }

download() {
  local url="$1" out="$2"
  # 单一镜像前缀（如 https://gh-proxy.com/）时自动拼接 GitHub 地址
  if [[ -n "${MB_MIRROR:-}" ]]; then
    url="${MB_MIRROR%/}/${url}"
  fi
  echo "  下载: $url"
  curl -fsSL --retry 2 --connect-timeout 20 -o "$out" "$url"
}

# ---------------------------------------------------------------- deb / rpm
echo "[linux-pkg] 3/5 cargo-deb / cargo-generate-rpm…"
for tool in cargo-deb cargo-generate-rpm; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "[linux-pkg] 缺少 $tool。" >&2
    echo "  本地:  cargo install $tool" >&2
    echo "  CI:    taiki-e/install-action@v2 传 $tool" >&2
    exit 1
  fi
done

cargo deb --no-build
cargo generate-rpm
# 两个工具的默认输出目录随版本变过（debian/ 或 deb/），用 find 收敛
find "$CARGO_TARGET_DIR" -maxdepth 3 -name '*.deb' -print -quit | {
  read -r f; [[ -n "$f" ]] && mv -f "$f" "$DIST/${NAME_BASE}.deb"
}
find "$CARGO_TARGET_DIR" -maxdepth 3 -name '*.rpm' -print -quit | {
  read -r f; [[ -n "$f" ]] && mv -f "$f" "$DIST/${NAME_BASE}.rpm"
}
[[ -f "$DIST/${NAME_BASE}.deb" ]] || { echo "[linux-pkg] 没找到 .deb 产物" >&2; exit 1; }
[[ -f "$DIST/${NAME_BASE}.rpm" ]] || { echo "[linux-pkg] 没找到 .rpm 产物" >&2; exit 1; }

# ---------------------------------------------------------------- tar.gz 便携包
echo "[linux-pkg] 4/5 tar.gz 便携包…"
STAGE="$ROOT/build_target/portable"
rm -rf "$STAGE"
mkdir -p "$STAGE/$NAME_BASE"
cp "$TARGET/mbvue"   "$STAGE/$NAME_BASE/"
cp assets/icon.png   "$STAGE/$NAME_BASE/"
cat > "$STAGE/$NAME_BASE/README.txt" <<EOF
mbvue ${VER} (linux-x64) 便携版

  ./mbvue              运行（需要系统已装 libwebkit2gtk-4.1 / libgtk-3；
                       Ubuntu/Debian: sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0）
  --web <dir>          指定前端产物目录（默认使用内嵌资源）
  --url <url>          直接加载指定 URL

配置/日志落在 \$XDG_DATA_HOME/mbvue/（默认 ~/.local/share/mbvue/）。
EOF
tar -czf "$DIST/${NAME_BASE}.tar.gz" -C "$STAGE" "$NAME_BASE"

# ---------------------------------------------------------------- AppImage
echo "[linux-pkg] 5/5 AppImage（linuxdeploy）…"

APPDIR="$ROOT/build_target/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps"

cp "$TARGET/mbvue" "$APPDIR/usr/bin/"
cp packaging/mbvue.desktop "$APPDIR/usr/share/applications/mbvue.desktop"
cp assets/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/mbvue.png"
# AppImage 顶层约定：应用图标 + .DirIcon + desktop 软链
cp assets/icon.png "$APPDIR/mbvue.png"
ln -sf mbvue.png "$APPDIR/.DirIcon"
ln -sf usr/share/applications/mbvue.desktop "$APPDIR/mbvue.desktop"

# WebKit 多进程助手 + injected bundle：按**系统绝对路径**原样复制进 AppDir。
# WebKitGTK 内部按编译期前缀（/usr/lib/<triplet>/webkit2gtk-4.1/）找它们，
# AppRun 会把 $APPDIR/usr/lib 加进 LD_LIBRARY_PATH，保持相同相对布局才能
# 从 squashfs 内启动渲染/网络进程。Tauri bundler 同款做法。
TRIPLET="$(gcc -print-multiarch 2>/dev/null || echo x86_64-linux-gnu)"
COPIED_HELPERS=0
for base in "/usr/lib/$TRIPLET" /usr/lib64 /usr/lib /usr/libexec; do
  for f in WebKitNetworkProcess WebKitWebProcess injected-bundle/libwebkit2gtkinjectedbundle.so; do
    src="$base/webkit2gtk-4.1/$f"
    if [[ -f "$src" ]]; then
      mkdir -p "$APPDIR/$(dirname "${src#/}")"
      cp "$src" "$APPDIR/${src#/}"
      COPIED_HELPERS=$((COPIED_HELPERS + 1))
    fi
  done
done
echo "[linux-pkg] WebKit 助手文件已复制 $COPIED_HELPERS 个（WebProcess/NetworkProcess/injected-bundle）"

# AppRun：用支持 apprun-hooks 的预编译版（linuxdeploy 的 gtk 插件会往
# apprun-hooks/ 写环境脚本，AppRun 启动时 source）。Tauri 钉版同源。
APPRUN="$TOOLS/AppRun-$ARCH"
if [[ ! -f "$APPRUN" ]]; then
  download "${APPRUN_URL:-https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-$ARCH}" "$APPRUN"
fi
chmod +x "$APPRUN"
cp "$APPRUN" "$APPDIR/AppRun"

# linuxdeploy：扫描 AppDir/usr/bin 的 ELF 并把动态库依赖收进 AppDir。
# 不用 --plugin gtk：Ubuntu 26 的 gdk-pixbuf 目录结构让插件的
# copy_lib_tree 失败（cp: cannot stat 'gdk-pixbuf-2.0/2.10.0'）；
# 而纯 webview 应用不需要它打包的东西——不加载 GTK 图标（无需 pixbuf
# loaders）、只走本地 http（无需 gio TLS 模块）、无 GSettings 依赖。
LINUXDEPLOY="$TOOLS/linuxdeploy-$ARCH.AppImage"
if [[ ! -f "$LINUXDEPLOY" ]]; then
  download "${LINUXDEPLOY_URL:-https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage}" "$LINUXDEPLOY"
fi
chmod +x "$LINUXDEPLOY"

export APPIMAGE_EXTRACT_AND_RUN=1   # CI 无 libfuse，AppImage 一律解压运行
export ARCH="$ARCH"
export OUTPUT="$DIST/${NAME_BASE}.AppImage"
# AppImage 是四种格式里最脆的（WebKit 助手/插件/发行版差异），失败不拖垮
# 其它产物：deb/rpm/tar.gz 已在前面打出
if ! PATH="$TOOLS:$PATH" "$LINUXDEPLOY" \
      --appimage-extract-and-run \
      --verbosity=1 \
      --appdir "$APPDIR" \
      --output appimage; then
  echo "[linux-pkg] 警告：AppImage 打包失败（deb/rpm/tar.gz 已产出，详见上方 linuxdeploy 日志）" >&2
else
  [[ -f "$OUTPUT" ]] || echo "[linux-pkg] 警告：linuxdeploy 未报错但 AppImage 不存在" >&2
fi

# ---------------------------------------------------------------- 汇总
echo ""
echo "============================================================"
for f in "$DIST"/*; do
  printf '  %-40s %9s MB\n' "$(basename "$f")" "$(mb "$(stat -c%s "$f")")"
done
echo "============================================================"
