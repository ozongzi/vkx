#!/bin/sh
# vkx 安装脚本。
#
#   curl -fsSL https://yinli.tech/file/install.sh | sh
#
# 只做一件事：把 vkx 二进制放进 ~/.vkx/bin，并把它加进 PATH。
#
# 工具链不在这里装。vkx 自己按需下载——第一次 vkx build 取桌面要的那几个组件，
# 第一次 vkx build --target android 才取 Android 那几 GB。想一次备齐就跑
# `vkx fetch`。
set -eu

MIRROR=${VKX_MIRROR:-https://yinli.tech/file}
VKX_HOME=${VKX_HOME:-$HOME/.vkx}

die() { printf '\033[1;31m错误:\033[0m %s\n' "$1" >&2; exit 1; }
step() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }

case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux)  os=linux ;;
    *)      die "这个脚本只支持 macOS 和 Linux。Windows 请用 install.ps1" ;;
esac
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64)  arch=x64 ;;
    *) die "不支持的架构: $(uname -m)" ;;
esac
platform="$os-$arch"

command -v curl >/dev/null 2>&1 || die "缺少 curl"

step "取 vkx（${platform}）"
mkdir -p "$VKX_HOME/bin"
tmp=$(mktemp)
version=$(curl -fsSL "$MIRROR/vkx/version.txt") || die "取不到版本信息，检查网络或换 VKX_MIRROR"
curl -fL --progress-bar -o "$tmp" "$MIRROR/vkx/$version/vkx-$version-$platform" \
    || die "下载失败：$MIRROR/vkx/$version/vkx-$version-$platform"
chmod +x "$tmp"
mv "$tmp" "$VKX_HOME/bin/vkx"

step "接进 PATH"
line="export PATH=\"\$HOME/.vkx/bin:\$PATH\""
for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    [ -f "$rc" ] || continue
    grep -qF '.vkx/bin' "$rc" || printf '\n# vkx\n%s\n' "$line" >> "$rc"
done

printf '\n装好了：%s\n\n' "$VKX_HOME/bin/vkx"
printf '打开一个新终端，然后：\n\n'
printf '    vkx new mygame\n'
printf '    cd mygame\n'
printf '    vkx run\n\n'
printf '第一次 vkx run 会下载编译需要的工具链（几十 MB）。\n'
printf '想一次备齐（含 Android 那 1.1 GB）：vkx fetch\n'
printf '看环境齐不齐：vkx doctor\n'
