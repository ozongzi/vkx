#!/bin/sh
# 包发出去之前先证明它是能用的。
#
#   verify-pack.sh <包所在目录> <平台> <原始 staging 目录>
#
# 起一个支持 Range 的本地服务器，用 vkx 从包里取每一个组件，再和 staging 里的
# 原始内容逐字节比对。任何一段对不上就退出非零。
#
# 这一步值钱在：包是拼出来的，偏移算错一个字节就全废，而那种错误只有在读者
# 机器上才会暴露。
set -eu

OUT=$(cd "${1:?用法: verify-pack.sh <包目录> <平台> <staging>}" && pwd)
PLATFORM=${2:?}
STAGING=$(cd "${3:?}" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"; [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null || true' EXIT

# 镜像的目录结构要和线上一致：<根>/sdk/<平台>/
mkdir -p "$WORK/site/sdk/$PLATFORM"
cp "$OUT/sdk-$PLATFORM.pack" "$OUT/manifest.txt" "$WORK/site/sdk/$PLATFORM/"

PORT=8971
python3 "$HERE/rangeserver.py" "$PORT" "$WORK/site" &
SERVER=$!
sleep 1

# 用刚编好的 vkx（CI 里 cargo build 出来的），PATH 清空，证明不依赖外部工具
VKX=${VKX_BIN:-$HERE/../target/release/vkx}
[ -x "$VKX" ] || { echo "找不到 vkx：$VKX" >&2; exit 1; }

failed=0
for component in $(awk '/^component /{print $2}' "$OUT/manifest.txt"); do
    home="$WORK/home-$component"
    mkdir -p "$home"
    if ! env -i HOME="$home" VKX_MIRROR="http://127.0.0.1:$PORT" PATH= \
        "$VKX" fetch --component "$component" >"$WORK/$component.log" 2>&1; then
        echo "✗ $component 取不下来" >&2
        cat "$WORK/$component.log" >&2
        failed=1
        continue
    fi
    if diff -r "$STAGING/$component" "$home/.vkx/sdk/$component" >"$WORK/$component.diff" 2>&1; then
        printf '✓ %-10s %s 个文件，逐字节一致\n' "$component" \
            "$(find "$home/.vkx/sdk/$component" -type f | wc -l | tr -d ' ')" >&2
    else
        echo "✗ $component 内容对不上：" >&2
        head -10 "$WORK/$component.diff" >&2
        failed=1
    fi
done

[ "$failed" -eq 0 ] || { echo "包自检未通过，不发布。" >&2; exit 1; }
echo "包自检通过。" >&2
