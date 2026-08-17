#!/bin/sh
# vkx 环境安装器（macOS / Linux）
#
#   curl -fsSL https://yinli.tech/file/install.sh | sh
#
# 从镜像装齐一整套开发环境到 ~/.vkx，不需要 sudo，不碰系统目录：
#   vkx 本体、CMake、Ninja、slangc、JDK、Gradle、Android SDK/NDK、
#   MoltenVK（macOS）、以及 SDL3 等依赖的源码缓存。
#
# 环境变量：
#   VKX_MIRROR   镜像地址，默认见下面的 DEFAULT_MIRROR
#   VKX_HOME     安装目录，默认 ~/.vkx
#   VKX_FORCE=1  已装的组件也重新安装
#
# 参数：
#   --no-android   跳过 Android 相关组件（省约 5 GB）
#   --no-vkx       只装环境，不装 vkx 本体
#                  （自己开发 vkx 时用：留给 cargo install 的那份）

set -eu

DEFAULT_MIRROR="https://yinli.tech/file"
MIRROR=${VKX_MIRROR:-$DEFAULT_MIRROR}
MIRROR=${MIRROR%/}
HOME_DIR=${VKX_HOME:-$HOME/.vkx}
FORCE=${VKX_FORCE:-}
WITH_ANDROID=1
WITH_VKX=1

for arg in "$@"; do
    case "$arg" in
        --no-android) WITH_ANDROID=0 ;;
        --no-vkx) WITH_VKX=0 ;;
        --force) FORCE=1 ;;
        *) printf '未知参数: %s\n' "$arg" >&2; exit 2 ;;
    esac
done

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD=$(printf '\033[1m'); GREEN=$(printf '\033[1;32m'); YELLOW=$(printf '\033[1;33m')
    RED=$(printf '\033[1;31m'); DIM=$(printf '\033[2m'); OFF=$(printf '\033[0m')
else
    BOLD=''; GREEN=''; YELLOW=''; RED=''; DIM=''; OFF=''
fi

step() { printf '%s==>%s %s%s%s\n' "$GREEN" "$OFF" "$BOLD" "$1" "$OFF"; }
info() { printf '    %s\n' "$1"; }
warn() { printf '%s警告:%s %s\n' "$YELLOW" "$OFF" "$1"; }
die()  { printf '%s错误:%s %s\n' "$RED" "$OFF" "$1" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "缺少 curl"
command -v tar  >/dev/null 2>&1 || die "缺少 tar"

# --- 识别平台 --------------------------------------------------------------

case "$(uname -s)" in
    Darwin) OS=macos ;;
    Linux)  OS=linux ;;
    *) die "不支持的系统: $(uname -s)（Windows 请用 install.ps1）" ;;
esac
case "$(uname -m)" in
    arm64|aarch64) ARCH=aarch64 ;;
    x86_64|amd64)  ARCH=x86_64 ;;
    *) die "不支持的架构: $(uname -m)" ;;
esac
PLATFORM="$OS-$ARCH"
[ "$OS" = macos ] && [ "$ARCH" = aarch64 ] && PLATFORM="macos-arm64"

step "vkx 环境安装"
info "平台   $PLATFORM"
info "镜像   $MIRROR"
info "目录   $HOME_DIR"

if [ "$MIRROR" = "$DEFAULT_MIRROR" ] && [ "${VKX_MIRROR:-}" = "" ]; then
    warn "还在用占位镜像地址，请用 VKX_MIRROR 指定你自己的镜像"
fi

# --- 取清单 ----------------------------------------------------------------

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT INT TERM
mkdir -p "$HOME_DIR"

MANIFEST="$WORK/manifest.txt"
curl -fsSL -o "$MANIFEST" "$MIRROR/manifest.txt" \
    || die "取不到清单 $MIRROR/manifest.txt
       确认镜像地址可访问，或用 VKX_MIRROR=<地址> 重新运行。"

INSTALLED="$HOME_DIR/installed.txt"
[ -f "$INSTALLED" ] || : > "$INSTALLED"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# 已装且版本一致就跳过。
is_installed() {
    grep -q "^$1	$2\$" "$INSTALLED" 2>/dev/null
}

mark_installed() {
    grep -v "^$1	" "$INSTALLED" > "$INSTALLED.tmp" 2>/dev/null || : > "$INSTALLED.tmp"
    printf '%s\t%s\n' "$1" "$2" >> "$INSTALLED.tmp"
    mv "$INSTALLED.tmp" "$INSTALLED"
}

install_component() {
    name=$1; version=$2; path=$3; want_sha=$4; dest=$5

    if [ -z "$FORCE" ] && is_installed "$name" "$version"; then
        info "$(printf '%-24s %s' "$name" "$version  已是最新")"
        return 0
    fi

    step "$name $version"
    archive="$WORK/$(basename "$path")"
    curl -fL --progress-bar -o "$archive" "$MIRROR/$path" || die "下载失败: $MIRROR/$path"

    got_sha=$(sha256_of "$archive")
    if [ "$got_sha" != "$want_sha" ]; then
        die "$name 校验失败
       期望 $want_sha
       实际 $got_sha
       镜像上的文件可能损坏或被改动过。"
    fi

    target="$HOME_DIR/$dest"
    rm -rf "$target"
    mkdir -p "$target"
    tar -xzf "$archive" -C "$target" || die "$name 解压失败"
    rm -f "$archive"

    mark_installed "$name" "$version"
}

# --- 逐个安装 --------------------------------------------------------------

NDK_VERSION=""
COUNT=0
while IFS='	' read -r name platform version path sha dest; do
    [ -n "$name" ] || continue
    case "$name" in \#*) continue ;; esac

    # 只装本平台和平台无关的组件。
    [ "$platform" = "$PLATFORM" ] || [ "$platform" = "any" ] || continue

    if [ "$WITH_ANDROID" = 0 ]; then
        case "$name" in
            android-*|jdk|gradle|sdl-android) continue ;;
        esac
    fi
    if [ "$WITH_VKX" = 0 ] && [ "$name" = vkx ]; then
        continue
    fi

    install_component "$name" "$version" "$path" "$sha" "$dest"
    [ "$name" = "android-ndk" ] && NDK_VERSION=$version
    COUNT=$((COUNT + 1))
done < "$MANIFEST"

[ "$COUNT" -gt 0 ] || die "清单里没有适用于 $PLATFORM 的组件"

# --- Android SDK 的许可文件 --------------------------------------------------
# Gradle 在需要补装组件时会检查这些文件。SDK 是我们直接铺好的，
# 补上许可可以避免它误判成「未接受许可」而中断构建。
if [ "$WITH_ANDROID" = 1 ] && [ -d "$HOME_DIR/android/sdk" ]; then
    mkdir -p "$HOME_DIR/android/sdk/licenses"
    printf '\n8933bad161af4178b1185d1a37fbf41ea5269c55\nd56f5187479451eabf01fb78af6dfcb131a6481e\n24333f8a63b6825ea9c5514f83c2829b004d1fee\n' \
        > "$HOME_DIR/android/sdk/licenses/android-sdk-license"
    printf '\n84831b9409646a918e30573bab4c9c91346d8abd\n' \
        > "$HOME_DIR/android/sdk/licenses/android-sdk-preview-license"
fi

# --- 写 env.sh 并接进 shell --------------------------------------------------

ENV_FILE="$HOME_DIR/env.sh"
cat > "$ENV_FILE" <<EOF
# 由 vkx 安装器生成，重跑安装脚本会覆盖。
export VKX_HOME="$HOME_DIR"
export JAVA_HOME="\$VKX_HOME/tools/jdk"
export ANDROID_HOME="\$VKX_HOME/android/sdk"
EOF
[ -n "$NDK_VERSION" ] && printf 'export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/%s"\n' "$NDK_VERSION" >> "$ENV_FILE"
cat >> "$ENV_FILE" <<'EOF'
export PATH="$VKX_HOME/bin:$VKX_HOME/tools/cmake/bin:$VKX_HOME/tools/ninja:$VKX_HOME/tools/slang/bin:$VKX_HOME/tools/gradle/bin:$ANDROID_HOME/platform-tools:$JAVA_HOME/bin:$PATH"
EOF
# Vulkan 的运行期配置。三个变量都不是 DYLD_ 开头的，这一点是有意的：
# macOS 的 SIP 会在执行系统二进制时把 DYLD_* 全部剥掉，所以只要中间隔了一层
# /bin/sh（Makefile、npm script、CI 都会），靠 DYLD_LIBRARY_PATH 找库的方案就失效。
#
# 校验层的 json 里 library_path 写的是相对路径，loader 按 json 所在目录解析，
# 所以库本身不需要任何搜索路径。
cat >> "$ENV_FILE" <<'EOF'

# 校验层：Debug 构建靠它报出用错 Vulkan 的地方。ADD 而不是覆盖，
# 机器上原有的层（RenderDoc 之类）照旧可用。
export VK_ADD_LAYER_PATH="$VKX_HOME/tools/vulkan/share/vulkan/explicit_layer.d${VK_ADD_LAYER_PATH:+:$VK_ADD_LAYER_PATH}"
EOF
if [ "$OS" = macos ]; then
    # macOS 是唯一连 loader 都没有的平台（Windows/Linux 的 loader 由驱动提供）。
    # SDL 默认会按名字搜索 libvulkan，搜不到就退而直接加载 MoltenVK——那样就绕过了
    # loader，校验层也就无从插入。用绝对路径明确指定 loader，把这条链补全：
    #   程序 -> libvulkan.1.dylib -> 校验层 -> MoltenVK
    cat >> "$ENV_FILE" <<'EOF'
export SDL_VULKAN_LIBRARY="$VKX_HOME/tools/vulkan/lib/libvulkan.1.dylib"
export VK_DRIVER_FILES="$VKX_HOME/tools/vulkan/share/vulkan/icd.d/MoltenVK_icd.json"
EOF
fi

SOURCE_LINE=". \"$ENV_FILE\""
ADDED=""
for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    [ -f "$rc" ] || continue
    if ! grep -qF "$ENV_FILE" "$rc" 2>/dev/null; then
        printf '\n# vkx\n%s\n' "$SOURCE_LINE" >> "$rc"
        ADDED="$ADDED $rc"
    fi
done

# --- 装不进 ~/.vkx 的两样东西 ------------------------------------------------

MISSING=""

# 1. C++ 编译器。macOS 上必须用 Apple 的 SDK（许可不允许再分发）。
if [ "$OS" = macos ]; then
    if ! xcode-select -p >/dev/null 2>&1; then
        MISSING="1"
        warn "缺少 Xcode 命令行工具（macOS SDK 无法由第三方分发）"
        info "请执行：xcode-select --install"
        info "如果还要构建 iOS，需要从 App Store 安装完整的 Xcode。"
    fi
else
    # Linux 上 clang 可以自带，但链接仍需要 libc 的开发文件。
    if [ ! -e /usr/include/stdio.h ]; then
        MISSING="1"
        warn "缺少 libc 开发头文件"
        if command -v apt-get >/dev/null 2>&1; then info "请执行：sudo apt install build-essential"
        elif command -v dnf >/dev/null 2>&1;  then info "请执行：sudo dnf install gcc-c++ glibc-devel"
        elif command -v pacman >/dev/null 2>&1; then info "请执行：sudo pacman -S base-devel"
        else info "请用你的发行版包管理器安装 C/C++ 开发包"; fi
    fi
fi

# 2. Vulkan 驱动。ICD 由显卡驱动提供，装不进用户目录。
if [ "$OS" = linux ]; then
    if ! ls /usr/share/vulkan/icd.d/*.json >/dev/null 2>&1; then
        MISSING="1"
        warn "没有找到 Vulkan 驱动（ICD），显卡驱动里才有"
        if command -v apt-get >/dev/null 2>&1; then info "请执行：sudo apt install mesa-vulkan-drivers"
        else info "请安装你的显卡驱动对应的 Vulkan 支持包"; fi
    fi
fi

# --- 自检 ------------------------------------------------------------------
# 装完立刻验一遍，别等到用户第一次构建时才发现哪个包是坏的。

step "自检"
check_tool() {
    label=$1; shift
    if "$@" >/dev/null 2>&1; then
        info "$(printf '%-8s %s' "$label" "可用")"
    else
        MISSING=1
        warn "$label 装上了却跑不起来：$1"
    fi
}
if [ "$WITH_VKX" = 1 ]; then
    check_tool vkx "$HOME_DIR/bin/vkx" --version
fi
check_tool cmake  "$HOME_DIR/tools/cmake/bin/cmake" --version
check_tool ninja  "$HOME_DIR/tools/ninja/ninja" --version
check_tool slangc "$HOME_DIR/tools/slang/bin/slangc" -h
if [ "$WITH_ANDROID" = 1 ]; then
    check_tool java "$HOME_DIR/tools/jdk/bin/java" -version
fi

# --- 收尾 ------------------------------------------------------------------

printf '\n%s安装完成。%s\n\n' "$BOLD" "$OFF"
if [ -n "$ADDED" ]; then
    info "已把环境接进：$ADDED"
fi
info "当前这个终端里先执行： $SOURCE_LINE"
printf '\n'
printf '  %s\n' "vkx new mygame   # 新建工程"
printf '  %s\n' "cd mygame && vkx run"
printf '\n'
if [ -n "$MISSING" ]; then
    warn "上面还有需要你手动处理的东西，处理完重新运行本脚本即可。"
fi
printf '%s%s%s\n' "$DIM" "全部文件都在 ${HOME_DIR}，删掉这个目录就等于卸载干净。" "$OFF"
