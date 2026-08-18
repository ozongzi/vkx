#!/bin/sh
# 把 release workflow 发的 vkx 二进制摊成镜像该有的样子。
#
#   publish-vkx.sh <版本> <输出目录>          例：publish-vkx.sh 0.1.10 out
#
# release.yml 发的是按 Rust 目标三元组命名的压缩包，而 install.sh、install.ps1
# 和 vkx selfupdate 三个消费者要的都是「裸二进制 + 一个 version.txt」：
#
#   vkx/version.txt                         内容就是版本号，不带 v
#   vkx/<版本>/vkx-<版本>-<平台>[.exe]      平台名跟 fetch::platform() 一套
#
# 这一层翻译以前没人做，version.txt 更是三个读它的地方、一个写它的都没有——
# 镜像上 vkx/ 一直是空的，安装脚本从来就没跑通过。
set -eu

VERSION=${1:?用法: publish-vkx.sh <版本> <输出目录>}
mkdir -p "${2:?}"
OUT=$(cd "${2:?}" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)

VERSION=${VERSION#v}          # 传 v0.1.10 也认
TAG="v$VERSION"
REPO=${VKX_REPO:-ozongzi/vkx}
BASE="https://github.com/$REPO/releases/download/$TAG"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
DEST="$OUT/vkx/$VERSION"
mkdir -p "$DEST"

# <Rust 目标三元组> <vkx 平台名>
TARGETS="aarch64-apple-darwin:macos-arm64
x86_64-apple-darwin:macos-x64
x86_64-unknown-linux-gnu:linux-x64
aarch64-unknown-linux-gnu:linux-arm64
x86_64-pc-windows-msvc:windows-x64
aarch64-pc-windows-msvc:windows-arm64"

echo "== vkx ${VERSION}（从 ${REPO} 的 ${TAG}）" >&2
count=0
for pair in $TARGETS; do
    triple=${pair%%:*}
    platform=${pair#*:}
    case $platform in
        windows-*) archive="vkx-$TAG-$triple.zip";    binary=vkx.exe; suffix=.exe ;;
        *)         archive="vkx-$TAG-$triple.tar.gz"; binary=vkx;     suffix= ;;
    esac

    echo "   $platform" >&2
    curl -fsSL "$BASE/$archive" -o "$WORK/$archive"
    rm -rf "$WORK/x"; mkdir -p "$WORK/x"
    case $archive in
        *.zip) (cd "$WORK/x" && unzip -qo "$WORK/$archive") ;;
        *)     tar xzf "$WORK/$archive" -C "$WORK/x" ;;
    esac
    [ -f "$WORK/x/$binary" ] || { echo "$archive 里没有 $binary" >&2; exit 1; }
    cp "$WORK/x/$binary" "$DEST/vkx-$VERSION-$platform$suffix"
    chmod +x "$DEST/vkx-$VERSION-$platform$suffix"
    count=$((count + 1))
done

# 最后写 version.txt：写在最后，六个二进制才算都就位了。
# 先写它的话，中途失败会让镜像指向一个下不下来的版本。
printf '%s\n' "$VERSION" > "$OUT/vkx/version.txt"

# 安装脚本也放进镜像根，读者 curl 一个地址就够了。
cp "$HERE/../install/install.sh" "$HERE/../install/install.ps1" "$OUT/"

printf '\n%s 个平台就位：%s\n' "$count" "$OUT" >&2
printf '接下来把 %s/vkx/ 和两个安装脚本放到镜像根下（见 DEPLOY.md）。\n' "$OUT" >&2
