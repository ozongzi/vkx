#!/bin/sh
# 收 sources 组件：Jolt 和 GameNetworkingSockets 的源码。
#
#   build-sources.sh <输出目录>
#
# 这两个不预编译。它们的公开接口是 C++ 类，预编译的 .a 要和读者的标准库实现、
# 异常/RTTI 开关、Windows 运行时全部对齐，对不上就是链接失败或者运行时崩在
# std::string 的析构里。所以只放源码，vkx add 打开后在读者机器上编。
#
# 平台无关，所有平台的包共用同一份。
set -eu

mkdir -p "${1:?用法: build-sources.sh <输出目录>}"
OUT=$(cd "${1:?用法: build-sources.sh <输出目录>}" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/versions.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"
GH=https://github.com

get() {   # get <url> <目标目录名>
    echo "  取 $2" >&2
    curl -fsSL "$1" -o "$WORK/$2.tar.gz"
    mkdir -p "$OUT/$2"
    tar xzf "$WORK/$2.tar.gz" -C "$OUT/$2" --strip-components=1
}

echo "== Jolt $JOLT" >&2
get "$GH/jrouwe/JoltPhysics/archive/refs/tags/v$JOLT.tar.gz" jolt

echo "== GameNetworkingSockets $GAMENETWORKING" >&2
get "$GH/ValveSoftware/GameNetworkingSockets/archive/refs/tags/v$GAMENETWORKING.tar.gz" gamenetworking

# 源码包里有大量用不上的东西，砍掉能省下可观的下载量
for junk in Docs docs Samples samples UnitTests tests Tests examples .github; do
    rm -rf "$OUT"/*/"$junk"
done

printf '\nsources 组件建好了：%s\n' "$OUT" >&2
du -sh "$OUT" >&2
