#!/bin/sh
# SDK 包里每样东西的版本。改这里就是升级。
#
# 不追版本：vkx 的版本号就是这一整套的版本号，工程里不记任何依赖版本，
# 所以升级是原子的——换一个 vkx，换一整套包。

# 工具
SLANG=2026.14.1
CMAKE=4.1.2
NINJA=1.13.2
CLANG_FORMAT=22.1.8
LLVM_MINGW=20250910

# Vulkan
VULKAN_SDK=1.4.357          # loader + 校验层
VULKAN_HEADERS=1.4.313
VOLK=1.4.304
MOLTENVK=1.4.2

# 预编译的 C 库（我们自己编，保证同一套编译器、ABI 一致）
SDL=3.4.14
MBEDTLS=3.6.5
ZLIB=1.3.1
FREETYPE=2.14.1

# 只有头文件的（放进去就能 #include，不用编）
CPP_HTTPLIB=0.18.3
STB_COMMIT=f0569113c93ad095470c54bf34a17b36646bbbb5   # stb_image.h
GLM=1.0.1

# 要从源码编的（C++ ABI 不能预编译，vkx add 打开后在读者机器上编）
JOLT=5.2.0
GAMENETWORKING=1.4.1

# Android
JDK=21
GRADLE=8.13
NDK=28.2.13676358
