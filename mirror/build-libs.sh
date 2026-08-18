#!/bin/sh
# 编 SDK 包里的 libs 组件：预编译的 C 库 + 全部头文件。
#
#   build-libs.sh <平台> <输出目录>
#
# 输出目录长这样，解开就是 ~/.vkx/sdk/libs 该有的内容：
#
#   lib/      libSDL3.a libmbedtls.a libz.a libfreetype.a …
#   include/  SDL3/ mbedtls/ freetype/ glm/ httplib.h stb_image.h …
#   cmake/    各库的 config，生成的 CMakeLists 用 find_package 找它们
#
# 只编 C 库。Jolt 和 GameNetworkingSockets 的公开接口是 C++ 类，预编译的 .a
# 要和读者的标准库实现、异常/RTTI 开关全部对齐，对不上就崩在 std::string 的
# 析构里——那两个只放源码，由 vkx add 打开后在读者机器上编。
set -eu

PLATFORM=${1:?用法: build-libs.sh <平台> <输出目录>}
OUT=$(cd "$(dirname "${2:?}")" && pwd)/$(basename "$2")
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/versions.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT/lib" "$OUT/include" "$OUT/cmake"

GH=https://github.com

fetch() {   # fetch <url> <解开后的目录名>
    url=$1; name=$2
    echo "  取 $name" >&2
    curl -fsSL "$url" -o "$WORK/$name.tar.gz"
    mkdir -p "$WORK/$name"
    tar xzf "$WORK/$name.tar.gz" -C "$WORK/$name" --strip-components=1
}

# 交叉编译的参数由调用方通过 CMAKE_EXTRA 传进来（CI 里按平台填）。
CMAKE_EXTRA=${CMAKE_EXTRA:-}

cmake_build() {   # cmake_build <源码目录> <额外的 -D...>
    src=$1; shift
    # shellcheck disable=SC2086
    cmake -S "$src" -B "$src/build" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$OUT" \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DBUILD_SHARED_LIBS=OFF \
        $CMAKE_EXTRA "$@" >/dev/null
    cmake --build "$src/build" --parallel >/dev/null
    cmake --install "$src/build" >/dev/null
}

echo "== SDL3 $SDL" >&2
fetch "$GH/libsdl-org/SDL/releases/download/release-$SDL/SDL3-$SDL.tar.gz" sdl3
cmake_build "$WORK/sdl3" \
    -DSDL_SHARED=OFF -DSDL_STATIC=ON -DSDL_TEST_LIBRARY=OFF \
    -DSDL_EXAMPLES=OFF -DSDL_INSTALL=ON

echo "== zlib $ZLIB" >&2
fetch "$GH/madler/zlib/releases/download/v$ZLIB/zlib-$ZLIB.tar.gz" zlib
cmake_build "$WORK/zlib" -DZLIB_BUILD_EXAMPLES=OFF

echo "== mbedTLS $MBEDTLS" >&2
# mbedTLS 发的是 .tar.bz2，单独处理
curl -fsSL "$GH/Mbed-TLS/mbedtls/releases/download/mbedtls-$MBEDTLS/mbedtls-$MBEDTLS.tar.bz2" \
    -o "$WORK/mbedtls.tar.bz2"
mkdir -p "$WORK/mbedtls"
tar xjf "$WORK/mbedtls.tar.bz2" -C "$WORK/mbedtls" --strip-components=1
cmake_build "$WORK/mbedtls" \
    -DENABLE_TESTING=OFF -DENABLE_PROGRAMS=OFF -DMBEDTLS_FATAL_WARNINGS=OFF

echo "== FreeType $FREETYPE" >&2
# savannah 的镜像时不时 502，用 GitHub 上的官方仓库（标签是 VER-x-y-z 这种写法）
fetch "$GH/freetype/freetype/archive/refs/tags/VER-$(echo "$FREETYPE" | tr . -).tar.gz" freetype
# 所有可选依赖一律关掉，只留 zlib（用我们刚编的那份）。
# 不关的话 FreeType 会摸到构建机上的 libpng / harfbuzz，编出来的库依赖一个
# 不在包里的东西，读者链接时才报 Undefined symbols。PNG 支持只用于彩色 emoji
# 位图，用不上。
cmake_build "$WORK/freetype" \
    -DFT_DISABLE_HARFBUZZ=ON -DFT_DISABLE_BROTLI=ON -DFT_DISABLE_BZIP2=ON \
    -DFT_DISABLE_PNG=ON \
    -DFT_REQUIRE_ZLIB=ON -DCMAKE_PREFIX_PATH="$OUT"

# ---------------------------------------------------------------------------
# 只有头文件的：拷进去就能 #include，不用编也不用声明
# ---------------------------------------------------------------------------
echo "== cpp-httplib ${CPP_HTTPLIB}（仅头文件）" >&2
curl -fsSL "$GH/yhirose/cpp-httplib/raw/v$CPP_HTTPLIB/httplib.h" -o "$OUT/include/httplib.h"

echo "== stb_image（仅头文件）" >&2
curl -fsSL "$GH/nothings/stb/raw/$STB_COMMIT/stb_image.h" -o "$OUT/include/stb_image.h"
curl -fsSL "$GH/nothings/stb/raw/$STB_COMMIT/stb_image_write.h" -o "$OUT/include/stb_image_write.h"

echo "== GLM ${GLM}（仅头文件）" >&2
fetch "$GH/g-truc/glm/archive/refs/tags/$GLM.tar.gz" glm
cp -R "$WORK/glm/glm" "$OUT/include/glm"

# 许可证一并带上，读者分发游戏时用得到
mkdir -p "$OUT/licenses"
for pair in "sdl3 LICENSE.txt" "zlib LICENSE" "mbedtls LICENSE" "freetype LICENSE.TXT" "glm copying.txt"; do
    name=${pair%% *}; file=${pair#* }
    [ -f "$WORK/$name/$file" ] && cp "$WORK/$name/$file" "$OUT/licenses/$name.txt" || true
done

printf '\nlibs 组件建好了：%s\n' "$OUT" >&2
du -sh "$OUT" >&2
