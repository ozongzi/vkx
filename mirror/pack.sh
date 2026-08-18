#!/bin/sh
# 把一棵「组件目录树」打成 vkx 认的 SDK 包。
#
#   pack.sh <staging 目录> <平台> <输出目录>
#
# staging 下每个一级子目录就是一个组件：
#
#   staging/toolchain/  cmake ninja slangc clang-format
#   staging/libs/       预编译的 C 库 + 全部头文件
#   staging/vulkan/     loader、校验层、MoltenVK
#   staging/sources/    Jolt、GameNetworkingSockets 的源码
#   staging/android/    JDK、Gradle、SDK、NDK
#
# 每个组件各压一个 .tar.zst，再按顺序首尾相接成一个文件。
#
# 为什么不是 zip：zip 的中央目录在文件末尾，取中间一段拿到的字节不是合法 zip。
# 而 zstd 流可以首尾相接，每一段单独拿出来仍然能独立解开——于是 vkx 用一次
# HTTP Range 就能只取需要的那个组件。
#
# 组件顺序按「用到的先后」排：桌面构建要前三个，Android 那几 GB 排最后，
# 这样常见路径只需要读文件开头那一段。
set -eu

STAGING=${1:?用法: pack.sh <staging 目录> <平台> <输出目录>}
PLATFORM=${2:?}
OUT=${3:?}

ORDER="toolchain libs vulkan sources android"

mkdir -p "$OUT"
PACK="$OUT/sdk-$PLATFORM.pack"
MANIFEST="$OUT/manifest.txt"
: > "$PACK"

{
    echo "# vkx SDK 清单 —— $PLATFORM"
    echo "# 由 mirror/pack.sh 生成。offset/length 是这一段在 pack 里的字节范围。"
    echo "pack sdk-$PLATFORM.pack"
} > "$MANIFEST"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

describe() {
    case $1 in
        toolchain) echo "cmake / ninja / slangc / clang-format" ;;
        libs)      echo "预编译的 C 库和全部头文件" ;;
        vulkan)    echo "loader / 校验层 / MoltenVK" ;;
        sources)   echo "Jolt / GameNetworkingSockets 源码" ;;
        android)   echo "JDK / Gradle / SDK / NDK" ;;
        *)         echo "$1" ;;
    esac
}

offset=0
for component in $ORDER; do
    [ -d "$STAGING/$component" ] || continue

    piece="$OUT/.$component.tar.zst"
    # COPYFILE_DISABLE：macOS 的 bsdtar 默认会把扩展属性另存成 ._xxx 资源叉，
    # 解开之后满地都是垃圾文件，而且白占体积。
    #
    # -19 压得比 gzip 小一半，解压还更快；vkx 按魔数认格式，不用告诉它。
    COPYFILE_DISABLE=1 tar cf - -C "$STAGING/$component" . | zstd -19 -T0 -q -o "$piece" -f

    length=$(wc -c < "$piece" | tr -d ' ')
    sha=$(sha256_of "$piece")
    cat "$piece" >> "$PACK"
    rm -f "$piece"

    echo "component $component $offset $length $sha $(describe "$component")" >> "$MANIFEST"
    printf '  %-10s %10s 字节  偏移 %s\n' "$component" "$length" "$offset" >&2
    offset=$((offset + length))
done

printf '\n包: %s（%s 字节）\n清单: %s\n' "$PACK" "$offset" "$MANIFEST" >&2
