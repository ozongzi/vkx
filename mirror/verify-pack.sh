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
    # VKX_PLATFORM 必须显式给：交叉编出来的包（比如在 arm64 机器上编的
    # macos-x64）如果按本机平台去找，会去要一个根本没上传的清单。
    #
    # USERPROFILE 也要给：vkx 在 Windows 上认的是它，不是 HOME。env -i 把
    # 环境清空之后只设 HOME 的话，vkx 会退回当前目录，装到别处去——而且
    # fetch 本身还是成功的，只有后面 diff 才发现「目录不存在」。
    if ! env -i HOME="$home" USERPROFILE="$home" \
        VKX_MIRROR="http://127.0.0.1:$PORT" \
        VKX_PLATFORM="$PLATFORM" PATH= \
        "$VKX" fetch --component "$component" >"$WORK/$component.log" 2>&1; then
        echo "✗ $component 取不下来" >&2
        cat "$WORK/$component.log" >&2
        failed=1
        continue
    fi
    # 空目录和空目录 diff 是会通过的。真出过这种事：某个组件因为过滤器
    # 没匹配上，一个文件都没收进来，打出来 88 字节的空包，自检照样报绿。
    # 所以先证明「有东西」，再证明「一样」。
    # fetch 说成功、但东西不在预期位置，是最难查的一种：diff 会报「目录不
    # 存在」，看起来像是内容对不上。先把这两种情况分开。
    if [ ! -d "$home/.vkx/sdk/$component" ]; then
        echo "✗ $component: vkx 报告成功，但 $home/.vkx/sdk/$component 不存在" >&2
        echo "   （多半是 vkx 认的 home 环境变量和这里设的不是同一个）" >&2
        failed=1
        continue
    fi
    count=$(find "$STAGING/$component" -type f 2>/dev/null | wc -l | tr -d ' ')
    if [ "$count" -eq 0 ]; then
        echo "✗ $component 在 staging 里是空的——比对无意义，当作失败" >&2
        failed=1
        continue
    fi
    if diff -r "$STAGING/$component" "$home/.vkx/sdk/$component" >"$WORK/$component.diff" 2>&1; then
        printf '✓ %-10s %s 个文件，逐字节一致\n' "$component" "$count" >&2
    else
        echo "✗ $component 内容对不上：" >&2
        head -10 "$WORK/$component.diff" >&2
        failed=1
    fi
done

[ "$failed" -eq 0 ] || { echo "包自检未通过，不发布。" >&2; exit 1; }
echo "包自检通过。" >&2
