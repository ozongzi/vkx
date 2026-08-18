#!/usr/bin/env bash
# 把 vkx 需要的全部上游依赖同步成一棵可直接对外提供的镜像目录树。
#
#   ./sync.sh [输出目录]                    # 默认 ./mirror-root
#   ./sync.sh out --only slang              # 只同步某个组件
#   ./sync.sh out --platform macos-arm64    # 只同步某个平台（any 的照样同步）
#   ./sync.sh out --skip android-ndk,jdk    # 跳过某些组件
#   VKX_LOCAL_BIN=target/release/vkx ./sync.sh out   # 把本机编好的 vkx 放进镜像
#
# 产出：
#   <输出目录>/manifest.txt                          安装脚本读的清单
#   <输出目录>/<组件>/<版本>/<组件>-<版本>-<平台>.tar.gz
#
# 每个包都被重新打成统一格式：解开后的内容直接就是安装目录里该有的东西，
# 不需要再 strip 层级。安装脚本因此只要「下载 -> 校验 -> 解压到 dest」三步。
#
# 同步完把整棵树 rsync 到服务器，让它以 HTTPS 提供即可：
#   rsync -av --delete mirror-root/ user@host:/var/www/file/

set -euo pipefail

OUT=${1:-./mirror-root}
shift || true
# --only / --platform 收的是逗号分隔的列表；空列表表示「全都要」。
# 之前这里是单名精确匹配，传 a,b,c 会一个都不匹配——而且是静默的，
# 产出一个空包，下游的 diff 拿空目录比空目录还会「通过」。
in_list() {
    [ -n "$1" ] || return 0
    _rest=$1
    while [ -n "$_rest" ]; do
        case $_rest in
            *,*) _head=${_rest%%,*}; _rest=${_rest#*,} ;;
            *)   _head=$_rest;       _rest= ;;
        esac
        [ "$_head" != "$2" ] || return 0
    done
    return 1
}

ONLY=""
PLATFORM_FILTER=""
SKIP=""
while [ $# -gt 0 ]; do
    case "$1" in
        --only)     ONLY=${2:-}; shift 2 ;;
        --platform) PLATFORM_FILTER=${2:-}; shift 2 ;;
        --skip)     SKIP=${2:-}; shift 2 ;;
        *) echo "未知参数: $1" >&2; exit 2 ;;
    esac
done

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$OUT"
MANIFEST="$OUT/manifest.txt"
: > "$MANIFEST.new"
# 记录「上游 URL -> 镜像里的路径」，用来给多平台共用的包去重。
URLMAP="$WORK/urlmap"
: > "$URLMAP"

log()  { printf '\033[1;32m==>\033[0m %s\n' "$1"; }
info() { printf '    %s\n' "$1"; }
die()  { printf '\033[1;31m错误:\033[0m %s\n' "$1" >&2; exit 1; }

sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# fetch <组件> <版本> <平台> <上游URL> <安装目标> [源包内要取的子路径]
#
# 平台用 any 表示与平台无关。安装目标是相对 ~/.vkx 的路径。
fetch() {
    local name=$1 version=$2 platform=$3 url=$4 dest=$5 pick=${6:-}
    in_list "$ONLY" "$name" || return 0
    [ "$platform" = any ] || in_list "$PLATFORM_FILTER" "$platform" || return 0
    case ",$SKIP," in *",$name,"*) return 0 ;; esac

    # 同一个上游文件常常服务多个平台（比如 macOS 的包是通用二进制，
    # arm64 和 x86_64 共用）。已经打过包的就直接复用，不再存第二份。
    local seen
    # grep 找不到会返回 1，配上 set -e 会直接终止脚本，所以要兜住。
    seen=$(grep -F "$url	" "$URLMAP" 2>/dev/null | head -1 | cut -f2 || true)
    if [ -n "$seen" ]; then
        info "$name ($platform) 复用 $seen"
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$name" "$platform" "$version" "$seen" "$(sha256 "$OUT/$seen")" "$dest" >> "$MANIFEST.new"
        return 0
    fi

    local out_rel="$name/$version/$name-$version-$platform.tar.gz"
    local out_abs="$OUT/$out_rel"

    if [ -f "$out_abs" ]; then
        info "已有 $out_rel"
        printf '%s\t%s\n' "$url" "$out_rel" >> "$URLMAP"
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$name" "$platform" "$version" "$out_rel" "$(sha256 "$out_abs")" "$dest" >> "$MANIFEST.new"
        return 0
    fi

    log "$name $version ($platform)"
    local dir="$WORK/$name-$platform"
    rm -rf "$dir"; mkdir -p "$dir/dl" "$dir/x"

    local archive="$dir/dl/${url##*/}"
    case "$archive" in *\?*) archive="$dir/dl/download";; esac
    info "下载 $url"
    curl -fL --progress-bar -o "$archive" "$url" || die "下载失败: $url"

    # 解开上游包。上游有 zip 也有 tar.*，统一交给 tar / unzip。
    case "$archive" in
        *.zip) (cd "$dir/x" && unzip -q "$archive") ;;
        *)     tar -xf "$archive" -C "$dir/x" 2>/dev/null || (cd "$dir/x" && unzip -q "$archive") ;;
    esac

    # 定位真正要打包的目录：先按 pick 取子路径，再自动剥掉单层外壳。
    local root="$dir/x"
    if [ -n "$pick" ]; then
        root=$(find "$dir/x" -maxdepth 4 -type d -path "*/$pick" | head -1)
        [ -n "$root" ] || die "$name: 包里找不到 $pick"
    else
        while [ "$(find "$root" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" = "1" ] \
              && [ -d "$(find "$root" -mindepth 1 -maxdepth 1)" ]; do
            root=$(find "$root" -mindepth 1 -maxdepth 1)
        done
    fi

    # zip 常常丢掉可执行位，补回来。
    find "$root" -type d -name bin -exec sh -c 'chmod +x "$1"/* 2>/dev/null || true' _ {} \;
    find "$root" -type f -name '*.sh' -exec chmod +x {} + 2>/dev/null || true

    mkdir -p "$(dirname "$out_abs")"
    info "打包 $out_rel"
    tar -czf "$out_abs" -C "$root" .

    printf '%s\t%s\n' "$url" "$out_rel" >> "$URLMAP"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$name" "$platform" "$version" "$out_rel" "$(sha256 "$out_abs")" "$dest" >> "$MANIFEST.new"

    # 打完包立刻清掉解压出来的东西。NDK 这种解开有好几 GB，
    # 攒到脚本结束才清会把磁盘撑爆。
    rm -rf "$dir"
}

# ===========================================================================
# 组件清单
# ===========================================================================
# 版本集中写在这里，升级只改这一段。

SLANG=2026.14.1
CMAKE=4.1.2
NINJA=1.13.2
GRADLE=8.13
JDK=21
MOLTENVK=1.4.2
VULKAN_SDK=1.4.357        # loader + 校验层，见下面 vulkan-sdk 组件的说明
CLANG_FORMAT=22.1.8      # 代码格式化，见下面 clang-format 组件的说明
LLVM_MINGW=20250910
SDL=3.4.14
VULKAN_HEADERS=1.4.313
VOLK=1.4.304

NDK=28.2.13676358          # 对应上游文件名里的 r28c
NDK_FILE=r28c
# build-tools 的上游文件名用 r36.1，但包里的真实版本号是 36.1.0，
# 安装目录必须用后者——AGP 是按精确的版本目录名去找的。
BUILD_TOOLS_FILE=r36.1
BUILD_TOOLS=36.1.0
ANDROID_PLATFORM=36        # = Android 16，对应模版里的 compileSdk 36
CMDLINE_TOOLS=11076708

GH=https://github.com
DL=https://dl.google.com/android/repository

# --- Slang（着色器编译器）--------------------------------------------------
fetch slang "$SLANG" macos-arm64    "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-macos-aarch64.tar.gz"   tools/slang
fetch slang "$SLANG" macos-x86_64   "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-macos-x86_64.tar.gz"    tools/slang
fetch slang "$SLANG" linux-x86_64   "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-linux-x86_64.tar.gz"    tools/slang
fetch slang "$SLANG" linux-aarch64  "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-linux-aarch64.tar.gz"   tools/slang
fetch slang "$SLANG" windows-x86_64 "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-windows-x86_64.zip"     tools/slang
fetch slang "$SLANG" windows-aarch64 "$GH/shader-slang/slang/releases/download/v$SLANG/slang-$SLANG-windows-aarch64.zip"   tools/slang

# --- CMake -----------------------------------------------------------------
fetch cmake "$CMAKE" macos-arm64    "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-macos-universal.tar.gz"      tools/cmake CMake.app/Contents
fetch cmake "$CMAKE" macos-x86_64   "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-macos-universal.tar.gz"      tools/cmake CMake.app/Contents
fetch cmake "$CMAKE" linux-x86_64   "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-linux-x86_64.tar.gz"         tools/cmake
fetch cmake "$CMAKE" linux-aarch64  "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-linux-aarch64.tar.gz"        tools/cmake
fetch cmake "$CMAKE" windows-x86_64 "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-windows-x86_64.zip"          tools/cmake
fetch cmake "$CMAKE" windows-aarch64 "$GH/Kitware/CMake/releases/download/v$CMAKE/cmake-$CMAKE-windows-arm64.zip"          tools/cmake

# --- Ninja -----------------------------------------------------------------
fetch ninja "$NINJA" macos-arm64    "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-mac.zip"            tools/ninja
fetch ninja "$NINJA" macos-x86_64   "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-mac.zip"            tools/ninja
fetch ninja "$NINJA" linux-x86_64   "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-linux.zip"          tools/ninja
fetch ninja "$NINJA" linux-aarch64  "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-linux-aarch64.zip"  tools/ninja
fetch ninja "$NINJA" windows-x86_64 "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-win.zip"            tools/ninja
fetch ninja "$NINJA" windows-aarch64 "$GH/ninja-build/ninja/releases/download/v$NINJA/ninja-winarm64.zip"      tools/ninja

# --- JDK（Gradle 需要）------------------------------------------------------
TEM=https://api.adoptium.net/v3/binary/latest/$JDK/ga
fetch jdk "$JDK" macos-arm64     "$TEM/mac/aarch64/jdk/hotspot/normal/eclipse"     tools/jdk Contents/Home
fetch jdk "$JDK" macos-x86_64    "$TEM/mac/x64/jdk/hotspot/normal/eclipse"         tools/jdk Contents/Home
fetch jdk "$JDK" linux-x86_64    "$TEM/linux/x64/jdk/hotspot/normal/eclipse"       tools/jdk
fetch jdk "$JDK" linux-aarch64   "$TEM/linux/aarch64/jdk/hotspot/normal/eclipse"   tools/jdk
fetch jdk "$JDK" windows-x86_64  "$TEM/windows/x64/jdk/hotspot/normal/eclipse"     tools/jdk
fetch jdk "$JDK" windows-aarch64 "$TEM/windows/aarch64/jdk/hotspot/normal/eclipse" tools/jdk

# --- Gradle（与平台无关）----------------------------------------------------
fetch gradle "$GRADLE" any "https://services.gradle.org/distributions/gradle-$GRADLE-bin.zip" tools/gradle

# --- Android SDK ------------------------------------------------------------
fetch android-cmdline-tools "$CMDLINE_TOOLS" macos-arm64     "$DL/commandlinetools-mac-${CMDLINE_TOOLS}_latest.zip"   android/sdk/cmdline-tools/latest
fetch android-cmdline-tools "$CMDLINE_TOOLS" macos-x86_64    "$DL/commandlinetools-mac-${CMDLINE_TOOLS}_latest.zip"   android/sdk/cmdline-tools/latest
fetch android-cmdline-tools "$CMDLINE_TOOLS" linux-x86_64    "$DL/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip" android/sdk/cmdline-tools/latest
fetch android-cmdline-tools "$CMDLINE_TOOLS" linux-aarch64   "$DL/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip" android/sdk/cmdline-tools/latest
fetch android-cmdline-tools "$CMDLINE_TOOLS" windows-x86_64  "$DL/commandlinetools-win-${CMDLINE_TOOLS}_latest.zip"   android/sdk/cmdline-tools/latest
fetch android-cmdline-tools "$CMDLINE_TOOLS" windows-aarch64 "$DL/commandlinetools-win-${CMDLINE_TOOLS}_latest.zip"   android/sdk/cmdline-tools/latest

fetch android-platform-tools latest macos-arm64     "$DL/platform-tools-latest-darwin.zip"  android/sdk/platform-tools
fetch android-platform-tools latest macos-x86_64    "$DL/platform-tools-latest-darwin.zip"  android/sdk/platform-tools
fetch android-platform-tools latest linux-x86_64    "$DL/platform-tools-latest-linux.zip"   android/sdk/platform-tools
fetch android-platform-tools latest linux-aarch64   "$DL/platform-tools-latest-linux.zip"   android/sdk/platform-tools
fetch android-platform-tools latest windows-x86_64  "$DL/platform-tools-latest-windows.zip" android/sdk/platform-tools
fetch android-platform-tools latest windows-aarch64 "$DL/platform-tools-latest-windows.zip" android/sdk/platform-tools

fetch android-build-tools "$BUILD_TOOLS" macos-arm64     "$DL/build-tools_${BUILD_TOOLS_FILE}_macosx.zip"  "android/sdk/build-tools/$BUILD_TOOLS"
fetch android-build-tools "$BUILD_TOOLS" macos-x86_64    "$DL/build-tools_${BUILD_TOOLS_FILE}_macosx.zip"  "android/sdk/build-tools/$BUILD_TOOLS"
fetch android-build-tools "$BUILD_TOOLS" linux-x86_64    "$DL/build-tools_${BUILD_TOOLS_FILE}_linux.zip"   "android/sdk/build-tools/$BUILD_TOOLS"
fetch android-build-tools "$BUILD_TOOLS" linux-aarch64   "$DL/build-tools_${BUILD_TOOLS_FILE}_linux.zip"   "android/sdk/build-tools/$BUILD_TOOLS"
fetch android-build-tools "$BUILD_TOOLS" windows-x86_64  "$DL/build-tools_${BUILD_TOOLS_FILE}_windows.zip" "android/sdk/build-tools/$BUILD_TOOLS"
fetch android-build-tools "$BUILD_TOOLS" windows-aarch64 "$DL/build-tools_${BUILD_TOOLS_FILE}_windows.zip" "android/sdk/build-tools/$BUILD_TOOLS"

fetch android-platform "$ANDROID_PLATFORM" any "$DL/platform-${ANDROID_PLATFORM}_r01.zip" "android/sdk/platforms/android-$ANDROID_PLATFORM"

fetch android-ndk "$NDK" macos-arm64     "$DL/android-ndk-$NDK_FILE-darwin.zip"  "android/sdk/ndk/$NDK"
fetch android-ndk "$NDK" macos-x86_64    "$DL/android-ndk-$NDK_FILE-darwin.zip"  "android/sdk/ndk/$NDK"
fetch android-ndk "$NDK" linux-x86_64    "$DL/android-ndk-$NDK_FILE-linux.zip"   "android/sdk/ndk/$NDK"
fetch android-ndk "$NDK" windows-x86_64  "$DL/android-ndk-$NDK_FILE-windows.zip" "android/sdk/ndk/$NDK"

# --- Apple 平台的 Vulkan 实现 ------------------------------------------------
# 这里取的是 MoltenVK 的完整包，iOS 构建要用里面的静态库（static/）。
# 桌面 macOS 运行时用的那个 dylib 由下面的 vulkan-sdk 组件提供，版本和
# loader、校验层对齐。
fetch moltenvk "$MOLTENVK" macos-arm64  "$GH/KhronosGroup/MoltenVK/releases/download/v$MOLTENVK/MoltenVK-all.tar" tools/moltenvk MoltenVK/MoltenVK
fetch moltenvk "$MOLTENVK" macos-x86_64 "$GH/KhronosGroup/MoltenVK/releases/download/v$MOLTENVK/MoltenVK-all.tar" tools/moltenvk MoltenVK/MoltenVK

# --- Vulkan loader 与校验层 --------------------------------------------------
# 校验层是 Debug 构建能报出「你用错了 Vulkan」的唯一途径，它不在显卡驱动里，
# 必须自己分发。上游只有 LunarG 的整包（每平台 274~493 MB，而且 macOS /
# Windows 版是跑不动的 Qt 安装器），所以由 .github/workflows/vulkan-sdk.yml
# 挑出需要的几个文件重打包成几十 MB，发到一个单独的 Release。
# LunarG 不提供 Linux ARM64，那个平台在同一个 workflow 里从源码构建。
# loader 和校验层都是 Apache-2.0。
#
# 各平台拿到的东西不一样：
#   macOS            loader + MoltenVK dylib + 校验层（这平台什么都没有）
#   Linux / Windows  只有校验层，loader 和 ICD 由显卡驱动提供
# macOS 的库是 universal，两个平台共用一个包（fetch 会自动去重）。
VKSDK="$GH/ozongzi/vkx/releases/download/vulkan-sdk-$VULKAN_SDK/vulkan-sdk-$VULKAN_SDK"
fetch vulkan-sdk "$VULKAN_SDK" macos-arm64      "$VKSDK-macos-universal.tar.gz" tools/vulkan
fetch vulkan-sdk "$VULKAN_SDK" macos-x86_64     "$VKSDK-macos-universal.tar.gz" tools/vulkan
fetch vulkan-sdk "$VULKAN_SDK" linux-x86_64     "$VKSDK-linux-x86_64.tar.gz"    tools/vulkan
fetch vulkan-sdk "$VULKAN_SDK" linux-aarch64    "$VKSDK-linux-aarch64.tar.gz"   tools/vulkan
fetch vulkan-sdk "$VULKAN_SDK" windows-x86_64   "$VKSDK-windows-x86_64.tar.gz"  tools/vulkan
fetch vulkan-sdk "$VULKAN_SDK" windows-aarch64  "$VKSDK-windows-aarch64.tar.gz" tools/vulkan


# --- clang-format（代码格式化）---------------------------------------------
# 上游只以 PyPI wheel 的形式提供单独的二进制（LLVM 官方包是几百 MB 的整套工具链，
# 里面九成用不上）。wheel 本身就是 zip，解开取 clang_format/data/bin 那一层即可。
# 版本和 LLVM 对齐，配置文件在工程根目录的 .clang-format。
fetch clang-format "$CLANG_FORMAT" macos-arm64      "https://files.pythonhosted.org/packages/2e/55/539cc1036dae16659f50500ca34838cc5b16cd3e98e3faaf164186b98093/clang_format-22.1.8-py2.py3-none-macosx_11_0_arm64.whl" tools/clang-format clang_format/data/bin
fetch clang-format "$CLANG_FORMAT" macos-x86_64     "https://files.pythonhosted.org/packages/5d/d8/29b9db6098da1a011ca3f7560c3942fa81404dbbb4367c3bd1d5c435da3b/clang_format-22.1.8-py2.py3-none-macosx_10_9_x86_64.whl" tools/clang-format clang_format/data/bin
fetch clang-format "$CLANG_FORMAT" linux-x86_64     "https://files.pythonhosted.org/packages/e5/88/b82c066fa807da4ca2518fecf79071361f6324b77375e5e92c059c0697fd/clang_format-22.1.8-py2.py3-none-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl" tools/clang-format clang_format/data/bin
fetch clang-format "$CLANG_FORMAT" linux-aarch64    "https://files.pythonhosted.org/packages/50/25/a9734da014eecc1f54c051ad643a28f2f6643dcc812ac59320e80e2b1a3b/clang_format-22.1.8-py2.py3-none-manylinux_2_26_aarch64.manylinux_2_28_aarch64.whl" tools/clang-format clang_format/data/bin
fetch clang-format "$CLANG_FORMAT" windows-x86_64   "https://files.pythonhosted.org/packages/08/60/c6783b3190a8f741107a44912a11c39c1a51e254e86a4c43cb0151cea0dd/clang_format-22.1.8-py2.py3-none-win_amd64.whl" tools/clang-format clang_format/data/bin
fetch clang-format "$CLANG_FORMAT" windows-aarch64  "https://files.pythonhosted.org/packages/43/ca/7e1fa4a6044c37c37356bb18fc938d2811754231e448aaffc192dc3774ce/clang_format-22.1.8-py2.py3-none-win_arm64.whl" tools/clang-format clang_format/data/bin

# --- Windows 上自带的 C++ 工具链（免装 Visual Studio）------------------------
fetch llvm-mingw "$LLVM_MINGW" windows-x86_64  "$GH/mstorsjo/llvm-mingw/releases/download/$LLVM_MINGW/llvm-mingw-$LLVM_MINGW-ucrt-x86_64.zip"  tools/llvm-mingw
fetch llvm-mingw "$LLVM_MINGW" windows-aarch64 "$GH/mstorsjo/llvm-mingw/releases/download/$LLVM_MINGW/llvm-mingw-$LLVM_MINGW-ucrt-aarch64.zip" tools/llvm-mingw

# --- 工程依赖的源码（预先放好，构建时离线可用）------------------------------
fetch sdl "$SDL" any "$GH/libsdl-org/SDL/releases/download/release-$SDL/SDL3-$SDL.tar.gz" src/sdl3
fetch sdl-android "$SDL" any "$GH/libsdl-org/SDL/releases/download/release-$SDL/SDL3-devel-$SDL-android.zip" src/sdl3-android
fetch vulkan-headers "$VULKAN_HEADERS" any "$GH/KhronosGroup/Vulkan-Headers/archive/refs/tags/v$VULKAN_HEADERS.tar.gz" src/vulkan-headers
fetch volk "$VOLK" any "$GH/zeux/volk/archive/refs/tags/$VOLK.tar.gz" src/volk

# --- vkx 自身 ---------------------------------------------------------------
# 由 release workflow 产出，同步时从 GitHub Release 取；也可以直接把
# 编好的包丢进 <输出目录>/vkx/<版本>/ 再重跑本脚本。
VKX_VERSION=${VKX_VERSION:-}
if [ -n "${VKX_LOCAL_BIN:-}" ]; then
    # 还没发过版时，直接把本机编好的 vkx 塞进镜像。
    [ -f "$VKX_LOCAL_BIN" ] || die "VKX_LOCAL_BIN 指向的文件不存在: $VKX_LOCAL_BIN"
    # 默认按本机平台标注；在别的机器上打包时用 VKX_LOCAL_PLATFORM 指定。
    if [ -n "${VKX_LOCAL_PLATFORM:-}" ]; then
        local_platform=$VKX_LOCAL_PLATFORM
    else
    case "$(uname -s)-$(uname -m)" in
        Darwin-arm64)  local_platform=macos-arm64 ;;
        Darwin-x86_64) local_platform=macos-x86_64 ;;
        Linux-x86_64)  local_platform=linux-x86_64 ;;
        Linux-aarch64) local_platform=linux-aarch64 ;;
        *) die "VKX_LOCAL_BIN 不支持当前平台，请用 VKX_LOCAL_PLATFORM 指定" ;;
    esac
    fi
    version=${VKX_VERSION:-0.1.0}
    out_rel="vkx/$version/vkx-$version-$local_platform.tar.gz"
    log "vkx $version (${local_platform}，来自本机)"
    mkdir -p "$OUT/$(dirname "$out_rel")"
    tmp="$WORK/vkxbin"; rm -rf "$tmp"; mkdir -p "$tmp"
    cp "$VKX_LOCAL_BIN" "$tmp/vkx"; chmod +x "$tmp/vkx"
    tar -czf "$OUT/$out_rel" -C "$tmp" vkx
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        vkx "$local_platform" "$version" "$out_rel" "$(sha256 "$OUT/$out_rel")" bin >> "$MANIFEST.new"
elif [ -n "$VKX_VERSION" ]; then
    R="$GH/ozongzi/vkx/releases/download/v$VKX_VERSION"
    fetch vkx "$VKX_VERSION" macos-arm64     "$R/vkx-v$VKX_VERSION-aarch64-apple-darwin.tar.gz"      bin
    fetch vkx "$VKX_VERSION" macos-x86_64    "$R/vkx-v$VKX_VERSION-x86_64-apple-darwin.tar.gz"       bin
    fetch vkx "$VKX_VERSION" linux-x86_64    "$R/vkx-v$VKX_VERSION-x86_64-unknown-linux-gnu.tar.gz"  bin
    fetch vkx "$VKX_VERSION" linux-aarch64   "$R/vkx-v$VKX_VERSION-aarch64-unknown-linux-gnu.tar.gz" bin
    fetch vkx "$VKX_VERSION" windows-x86_64  "$R/vkx-v$VKX_VERSION-x86_64-pc-windows-msvc.zip"       bin
    fetch vkx "$VKX_VERSION" windows-aarch64 "$R/vkx-v$VKX_VERSION-aarch64-pc-windows-msvc.zip"      bin
fi

# 合并上一次的清单：这次没处理到的条目（被 --only / --platform / --skip
# 过滤掉的那些）原样保留，否则一次带过滤参数的同步就会把清单洗掉。
if [ -f "$MANIFEST" ]; then
    while IFS=$'\t' read -r old_name old_platform old_rest; do
        [ -n "$old_name" ] || continue
        if ! grep -q "^$old_name$(printf '\t')$old_platform$(printf '\t')" "$MANIFEST.new"; then
            printf '%s\t%s\t%s\n' "$old_name" "$old_platform" "$old_rest" >> "$MANIFEST.new"
        fi
    done < "$MANIFEST"
fi

sort -o "$MANIFEST.new" "$MANIFEST.new"
mv "$MANIFEST.new" "$MANIFEST"

# 安装脚本本身也放进镜像根目录，这样上传一次，用户 curl 一个地址就够了。
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
for script in install.sh install.ps1; do
    for candidate in "$SCRIPT_DIR/../install/$script" "$SCRIPT_DIR/$script"; do
        if [ -f "$candidate" ]; then
            cp "$candidate" "$OUT/$script"
            info "放入 $script"
            break
        fi
    done
done

log "完成"
info "清单: ${MANIFEST}（$(wc -l < "$MANIFEST" | tr -d ' ') 条）"
info "总量: $(du -sh "$OUT" | cut -f1)"
info "上传: rsync -av --delete $OUT/ user@host:/var/www/file/"
