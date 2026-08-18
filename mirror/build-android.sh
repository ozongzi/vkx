#!/bin/sh
# 从 sync.sh 铺好的镜像树里组装 android 组件。
#
#   build-android.sh <平台> <输出目录> [镜像根]
#
# 安卓这一整套——JDK、Gradle、cmdline-tools、platform-tools、build-tools、
# platform、NDK——全是上游现成的二进制，没有一行需要编译。所以它不该走那条
# 交叉编译的 CI 矩阵（那条流水线存在的理由是编 libs），更不该让流量绕经谁的
# 笔记本。镜像服务器上 sync.sh 早就把它们按平台打好 .tar.gz 放着了，这里直接
# 解出来重新摆成组件该有的样子。
#
# 组件解开后就是 ~/.vkx/sdk/android/ 的内容：
#
#   jdk/                    gradle/
#   sdk/cmdline-tools/latest    sdk/platform-tools    sdk/build-tools/<版本>
#   sdk/platforms/android-<版本>   sdk/ndk/<版本>
#
# 没有 NDK 的平台不出组件。Google 只为 macOS、linux-x86_64、windows-x86_64
# 发 NDK，ARM64 的 Linux/Windows 机器本来就构建不了 Android，把剩下那 580 MB
# 塞进包里只是让人白下。
set -eu

PLATFORM=${1:?用法: build-android.sh <平台> <输出目录> [镜像根]}
mkdir -p "${2:?}"
OUT=$(cd "${2:?}" && pwd)
ROOT=${3:-/var/www/file}
MANIFEST="$ROOT/manifest.txt"

[ -f "$MANIFEST" ] || { echo "找不到 ${MANIFEST}——镜像根给对了吗" >&2; exit 1; }

# sync.sh 的清单用 linux-x86_64 这套命名，vkx 用 linux-x64。
case $PLATFORM in
    macos-arm64)   SYNC=macos-arm64 ;;
    macos-x64)     SYNC=macos-x86_64 ;;
    linux-x64)     SYNC=linux-x86_64 ;;
    linux-arm64)   SYNC=linux-aarch64 ;;
    windows-x64)   SYNC=windows-x86_64 ;;
    windows-arm64) SYNC=windows-aarch64 ;;
    *) echo "不认识的平台: $PLATFORM" >&2; exit 1 ;;
esac

# 先看有没有 NDK。没有就别费劲了。
if ! awk -F'\t' -v p="$SYNC" '$1=="android-ndk" && $2==p {found=1} END{exit !found}' "$MANIFEST"; then
    echo "$PLATFORM 没有 NDK（上游不发），不出 android 组件" >&2
    exit 2
fi

# 清单第 4 列是包路径，第 6 列是旧布局里的安装目标。旧布局把 JDK 和 Gradle
# 放在 tools/ 下、SDK 放在 android/sdk 下；组件里统一收进 android/ 这一层。
awk -F'\t' -v p="$SYNC" '
    $2 != p && $2 != "any" { next }
    $1 ~ /^(jdk|gradle|android-)/ { print $1 "\t" $4 "\t" $6 }
' "$MANIFEST" | sort -u | while IFS="$(printf '\t')" read -r name path dest; do
    case $dest in
        tools/*)   inner=${dest#tools/} ;;      # tools/jdk    -> jdk
        android/*) inner=${dest#android/} ;;    # android/sdk/ndk/x -> sdk/ndk/x
        *)         inner=$dest ;;
    esac
    archive="$ROOT/$path"
    [ -f "$archive" ] || { echo "缺文件: $archive" >&2; exit 1; }
    printf '   %-24s -> %s\n' "$name" "$inner" >&2
    mkdir -p "$OUT/$inner"
    tar xzf "$archive" -C "$OUT/$inner"
done

printf '\nandroid 组件建好了：%s\n' "$OUT" >&2
du -sh "$OUT" >&2
