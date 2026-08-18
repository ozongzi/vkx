#!/bin/sh
# 收 toolchain 和 vulkan 两个组件：上游发的二进制，我们只挑文件重打包。
#
#   collect-prebuilt.sh <平台> <staging 目录>
#
# 和 build-libs.sh 的区别是这里不编译——cmake、ninja、slangc、clang-format、
# Vulkan loader、校验层、MoltenVK 上游都有现成的二进制。每一家的下载地址、压缩
# 格式和「包里哪一层才是真正要的目录」都不一样，这套规则已经在 sync.sh 里写全
# 了（比如 macOS 的 CMake 要剥掉 CMake.app/Contents，Windows 的 CMake 发的是
# zip 而不是 tar.gz）。所以这里只做一件事：调 sync.sh，然后把它产出的包摊到
# staging 的对应位置。规则只有一份，改一处就够。
set -eu

PLATFORM=${1:?用法: collect-prebuilt.sh <平台> <staging 目录>}
mkdir -p "${2:?}"
STAGING=$(cd "${2:?}" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$STAGING/toolchain" "$STAGING/vulkan"

# sync.sh 用的是 linux-x86_64 这套命名，vkx 要的是 linux-x64。
# 包名必须跟 vkx 走（它按自己的 platform() 去镜像上找），所以这里翻译一次。
case $PLATFORM in
    macos-arm64)   SYNC_PLATFORM=macos-arm64 ;;
    macos-x64)     SYNC_PLATFORM=macos-x86_64 ;;
    linux-x64)     SYNC_PLATFORM=linux-x86_64 ;;
    linux-arm64)   SYNC_PLATFORM=linux-aarch64 ;;
    windows-x64)   SYNC_PLATFORM=windows-x86_64 ;;
    windows-arm64) SYNC_PLATFORM=windows-aarch64 ;;
    *) echo "不认识的平台: $PLATFORM" >&2; exit 1 ;;
esac

echo "== 按 sync.sh 的规则取 cmake / ninja / slang / clang-format / vulkan" >&2
# 必须用 bash 调：sync.sh 的 shebang 是 bash，用 sh 调会把 shebang 覆盖掉，
# 而 Ubuntu 的 /bin/sh 是 dash，没有 pipefail，第 20 行就报错。
# macOS 和 Git Bash 的 /bin/sh 都是 bash 伪装，所以这个错只在 Linux 上出现。
bash "$HERE/sync.sh" "$WORK/mirror" --platform "$SYNC_PLATFORM" \
    --only cmake,ninja,slang,clang-format,llvm-mingw,vulkan-sdk,moltenvk >&2

# sync.sh 产出的是「解开即是安装目录内容」的包，直接摊进对应组件。
# moltenvk 只有 macOS 有，找不到就跳过——不是错误。
# llvm-mingw 只有 Windows 有，moltenvk 只有 macOS 有，其余平台会走「跳过」。
for pair in "cmake toolchain/cmake" "ninja toolchain/ninja" \
            "slang toolchain/slang" "clang-format toolchain/clang-format" \
            "llvm-mingw toolchain/llvm-mingw" \
            "vulkan-sdk vulkan/vulkan" "moltenvk vulkan/moltenvk"; do
    name=${pair%% *}; dest=${pair#* }
    archive=$(find "$WORK/mirror/$name" -name '*.tar.gz' 2>/dev/null | head -1) || true
    if [ -z "$archive" ]; then
        echo "   跳过 ${name}（这个平台没有）" >&2
        continue
    fi
    mkdir -p "$STAGING/$dest"
    tar xzf "$archive" -C "$STAGING/$dest"
done

# 少了这几个的话后面 vkx 一定跑不起来，宁可在这里炸。
MUST="toolchain/cmake toolchain/ninja toolchain/slang vulkan/vulkan"
case $PLATFORM in
    # Windows 上没有可用的系统编译器：MSVC 不许分发、也不许用（用了的话
    # SDK 里 mingw 编的 .a 和读者的 MSVC ABI 对不上）。所以编译器必须自带。
    windows-*) MUST="$MUST toolchain/llvm-mingw" ;;
esac
for must in $MUST; do
    if [ -z "$(ls -A "$STAGING/$must" 2>/dev/null)" ]; then
        echo "$PLATFORM: $must 是空的，sync.sh 没产出这个组件" >&2
        exit 1
    fi
done

printf '\ntoolchain 和 vulkan 组件就位\n' >&2
du -sh "$STAGING/toolchain" "$STAGING/vulkan" >&2
