#!/bin/sh
# 往已经打好的 SDK 包尾部追加一个组件。
#
#   append-component.sh <pack 文件> <manifest 文件> <组件名> <组件目录>
#
# 包就是各段首尾相接，清单记着每段的偏移和长度。所以追加一段等于 cat 一次、
# 清单加一行——不用把整个包重打。pack.sh 里 ORDER 把 android 排在最后，就是
# 为了这个：桌面读者用 Range 只取前面几段，永远不会碰到它。
#
# 压缩级别可以用 ZSTD_LEVEL 调，默认 19，和 pack.sh 一致。
#
# android 组件用 10。在镜像机（单核）上实测 300 MB 的 NDK 数据：
#
#   -19   57.6 MB   386 秒
#   -10   65.4 MB    25 秒
#   -6    69.0 MB    15 秒
#
# -19 只小 12%，却慢 15 倍。整个 android 组件 3.1 GB 换算下来是每平台省
# 81 MB、多花一个多小时，四个平台就是四小时。里面大半是已经压过的 NDK
# 二进制，再挤没多少油水。级别对 vkx 是不可见的——它按魔数认格式。
set -eu
LEVEL=${ZSTD_LEVEL:-19}

PACK=${1:?用法: append-component.sh <pack> <manifest> <组件名> <组件目录>}
MANIFEST=${2:?}
NAME=${3:?}
DIR=${4:?}

[ -f "$PACK" ] || { echo "找不到包: $PACK" >&2; exit 1; }
[ -d "$DIR" ]  || { echo "找不到组件目录: $DIR" >&2; exit 1; }

if grep -q "^component $NAME " "$MANIFEST"; then
    echo "清单里已经有 $NAME 这一段了，先去掉再追加" >&2
    exit 1
fi

describe() {
    case $1 in
        android) echo "JDK / Gradle / SDK / NDK" ;;
        *)       echo "$1" ;;
    esac
}

# 偏移就是当前包的长度——新的一段接在末尾。
offset=$(wc -c < "$PACK" | tr -d ' ')

piece="$PACK.$NAME.tmp"
COPYFILE_DISABLE=1 tar cf - -C "$DIR" . | zstd "-$LEVEL" -T0 -q -o "$piece" -f

length=$(wc -c < "$piece" | tr -d ' ')
if command -v sha256sum >/dev/null 2>&1; then
    sha=$(sha256sum "$piece" | cut -d' ' -f1)
else
    sha=$(shasum -a 256 "$piece" | cut -d' ' -f1)
fi

cat "$piece" >> "$PACK"
rm -f "$piece"
echo "component $NAME $offset $length $sha $(describe "$NAME")" >> "$MANIFEST"

printf '  %-10s %s 字节  偏移 %s（zstd -%s）\n' "$NAME" "$length" "$offset" "$LEVEL" >&2
printf '包现在 %s 字节\n' "$(wc -c < "$PACK" | tr -d ' ')" >&2
