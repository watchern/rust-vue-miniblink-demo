#!/usr/bin/env bash
# 下载 miniblink SDK（mb132_x64.dll + mb.h）
#
# GitHub 直连不通时，脚本会依次尝试若干镜像站。
# 也可以直接用环境变量指定：  MB_MIRROR=https://xxx/ ./fetch-miniblink.sh
#
# 产物：vendor/miniblink/{mb132_x64.dll, mb132_x32.dll, demo_src/mb.h}

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RELEASE_TAG="${MB_TAG:-20251212}"
ASSET="miniblink132_251212.7z"
REPO="weolar/miniblink49"

MIRRORS=(
  "${MB_MIRROR:-}"
  "https://gh-proxy.com/"
  "https://gh.llkk.cc/"
  "https://gh-proxy.net/"
  "https://ghps.cc/"
  "https://gh-proxy.org/"
)

mkdir -p vendor
TMP="vendor/miniblink.7z"

download() {
  local url="$1" out="$2"
  echo "  尝试: $url"
  curl -fsSL --retry 2 --connect-timeout 15 -m 900 -o "$out" "$url"
}

ok=false
for m in "${MIRRORS[@]}"; do
  [ -z "$m" ] && continue
  url="${m%/}/https://github.com/${REPO}/releases/download/${RELEASE_TAG}/${ASSET}"
  if download "$url" "$TMP"; then
    echo "  下载成功 ✓"
    ok=true
    break
  fi
  echo "  失败，换下一个镜像"
done

if [ "$ok" != true ]; then
  echo "所有镜像都失败了。可手动下载后放到 vendor/ 目录："
  echo "  https://github.com/${REPO}/releases/download/${RELEASE_TAG}/${ASSET}"
  exit 1
fi

# --- 解压：优先系统 7z，其次 python 的 py7zr ---
echo "解压 $TMP ..."
rm -rf vendor/miniblink
mkdir -p vendor/miniblink

if command -v 7z >/dev/null 2>&1; then
  7z x "$TMP" -ovendor/miniblink -y >/dev/null
elif command -v 7za >/dev/null 2>&1; then
  7za x "$TMP" -ovendor/miniblink -y >/dev/null
else
  python3 - <<'PY'
import sys
try:
    import py7zr
except ImportError:
    sys.exit("需要 7z 或 py7zr：pip install py7zr")
with py7zr.SevenZipFile("vendor/miniblink.7z", mode="r") as z:
    z.extractall(path="vendor/miniblink")
PY
fi

rm -f "$TMP"

echo
echo "完成，产物："
ls -lh vendor/miniblink/ | sed 's/^/  /'
