#!/bin/sh
# 收 toolchain 和 vulkan 两个组件：上游发的二进制，我们只挑文件重打包。
#
#   collect-prebuilt.sh <平台> <staging 目录>
#
# 和 build-libs.sh 的区别是这里不编译——cmake、ninja、slangc、clang-format、
# Vulkan loader、校验层、MoltenVK 上游都有现成的二进制，挑出需要的那几个文件
# 就行。挑的规则和取舍见 sync.sh 里对应组件的注释。
set -eu

PLATFORM=${1:?用法: collect-prebuilt.sh <平台> <staging 目录>}
STAGING=$(cd "$(dirname "${2:?}")" && pwd)/$(basename "$2")
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/versions.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$STAGING/toolchain" "$STAGING/vulkan"

GH=https://github.com

# sync.sh 用的是 linux-x86_64 这套命名，vkx 要的是 linux-x64。
# 包名必须跟 vkx 走（它按自己的 platform() 去镜像上找），所以这里翻译一次。
case $PLATFORM in
    macos-arm64)   SYNC_PLATFORM=macos-arm64 ;;
    macos-x64)     SYNC_PLATFORM=macos-x86_64 ;;
    linux-x64)     SYNC_PLATFORM=linux-x86_64 ;;
    linux-arm64)   SYNC_PLATFORM=linux-aarch64 ;;
    windows-x64)   SYNC_PLATFORM=windows-x86_64 ;;
    windows-arm64) SYNC_PLATFORM=windows-aarch64 ;;
esac

# 上游的文件名各家不一样，按平台翻译。
case $PLATFORM in
    macos-*)   CMAKE_SUFFIX="macos-universal";     NINJA_SUFFIX="mac"      ;;
    linux-x64) CMAKE_SUFFIX="linux-x86_64";        NINJA_SUFFIX="linux"    ;;
    linux-arm64) CMAKE_SUFFIX="linux-aarch64";     NINJA_SUFFIX="linux-aarch64" ;;
    windows-x64) CMAKE_SUFFIX="windows-x86_64";    NINJA_SUFFIX="win"      ;;
    windows-arm64) CMAKE_SUFFIX="windows-arm64";   NINJA_SUFFIX="winarm64" ;;
    *) echo "不认识的平台: $PLATFORM" >&2; exit 1 ;;
esac

echo "== CMake $CMAKE" >&2
curl -fsSL "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-$CMAKE_SUFFIX.tar.gz" \
    -o "$WORK/cmake.tar.gz"
mkdir -p "$STAGING/toolchain/cmake"
tar xzf "$WORK/cmake.tar.gz" -C "$STAGING/toolchain/cmake" --strip-components=1

echo "== Ninja $NINJA" >&2
curl -fsSL "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-$NINJA_SUFFIX.zip" \
    -o "$WORK/ninja.zip"
mkdir -p "$STAGING/toolchain/ninja"
(cd "$STAGING/toolchain/ninja" && unzip -qo "$WORK/ninja.zip")

# slangc、clang-format、Vulkan loader / 校验层 / MoltenVK 的下载和挑文件规则
# 已经在 sync.sh 里写好了，这里复用它，避免同一套逻辑维护两份。
echo "== slang / clang-format / vulkan（复用 sync.sh 的规则）" >&2
sh "$HERE/sync.sh" "$WORK/mirror" --platform "$SYNC_PLATFORM" \
    --only slang,clang-format,vulkan-sdk,moltenvk >&2

# sync.sh 产出的是「解开即是安装目录内容」的包，直接摊进对应组件。
for pair in "slang toolchain/slang" "clang-format toolchain/clang-format" \
            "vulkan-sdk vulkan/vulkan" "moltenvk vulkan/moltenvk"; do
    name=${pair%% *}; dest=${pair#* }
    archive=$(find "$WORK/mirror/$name" -name '*.tar.gz' | head -1) || true
    [ -n "$archive" ] || continue
    mkdir -p "$STAGING/$dest"
    tar xzf "$archive" -C "$STAGING/$dest"
done

printf '\ntoolchain 和 vulkan 组件就位\n' >&2
du -sh "$STAGING/toolchain" "$STAGING/vulkan" >&2
