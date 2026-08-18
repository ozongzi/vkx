# vkx 接管构建 —— 设计

vkx 不再往工程里塞一份要用户维护的 `CMakeLists.txt`。工程里只有 `vkx.toml`，
CMake 由 vkx 生成到 `target/`。

动机有三条：

- **教程的 diff 不会分叉。** 每一步都是带行号的 diff，只要有人手改过 CMakeLists，
  后面所有 CMake 的 diff 就全对不上。生成的文件没人改，也就不会分叉。
- **依赖按需加。** 不必为了「以后可能要用」把 Jolt、HarfBuzz 都先塞进模版，
  也就不必为用不到的库先趟通五个平台的交叉编译。
- **首次构建只编真正用到的东西。**

---

## 工程长什么样

```
mygame/
  vkx.toml              唯一要编辑的配置
  CMakePresets.json     生成的，给 IDE 认路
  .gitignore            生成的，忽略 target/
  src/                  自动收集
  shaders/              自动编译并嵌入
  assets/
  target/
    CMakeLists.txt      生成的，别手改
    debug/  release/    构建目录
    android/  ios/      各平台的工程壳
  dist/                 vkx dist 的产出
```

`target/` 整个不进版本库。删掉它 `vkx build` 会重新生成。

---

## vkx.toml 全部字段

```toml
[project]
name = "mygame"                  # 可执行文件名，同时是 CMake target 名
package_id = "com.moba.mygame"   # Android applicationId / iOS bundle id
version = "0.1.0"
cxx_standard = 20                # 可选，默认 20


# ---------------------------------------------------------------------------
# 要从源码编的库：只有这两个值得开关，因为它们各自是几分钟的编译时间。
# 预编译的 C 库和只有头文件的库永远可用，不用声明——想用 stb_image 直接 #include。
# ---------------------------------------------------------------------------
[libs]
jolt = false
gamenetworking = false

# ---------------------------------------------------------------------------
# 源码：默认递归收集 src/ 下的 .c .cc .cpp .cxx
# ---------------------------------------------------------------------------
[source]
dirs = ["src"]
exclude = ["src/experiments/**"]
include_dirs = ["src"]           # 默认等于 dirs，所以 #include "gpu/gpu.h" 直接可用

# ---------------------------------------------------------------------------
# 着色器：默认扫 shaders/*.slang，每个 [shader(...)] 入口各出一份 SPIR-V，
# 转成 C 数组嵌进二进制，头文件名是 <文件名>_<入口>.spv.h
# ---------------------------------------------------------------------------
[shaders]
dir = "shaders"
include_dirs = ["shaders"]       # 让 .slang 之间能 #include

# ---------------------------------------------------------------------------
# 任意文件嵌成 C 数组，生成 <名> 和 <名>_SIZE
# ---------------------------------------------------------------------------
[embed]
files = ["assets/font.bin"]

# ---------------------------------------------------------------------------
# 平台
# ---------------------------------------------------------------------------
[android]
min_sdk = 28
target_sdk = 35
abis = ["arm64-v8a"]

[ios]
development_team = "ABCDE12345"  # 填了才能出真机包
deployment_target = "16.0"

# ---------------------------------------------------------------------------
# 逃生舱：TOML 表达不了的事写在这里
# ---------------------------------------------------------------------------
[build]
cmake_include = "extra.cmake"    # 生成的 CMakeLists 末尾 include 它
defines = ["MY_FLAG=1"]
```

---

## 依赖的三种形态

镜像清单里每个库标明自己是哪一种，vkx 按形态决定生成什么。

| 形态 | 例子 | 生成的 CMake |
| --- | --- | --- |
| **预编译二进制**（C ABI 稳定） | SDL3、mbedTLS、zlib、FreeType | 指向 `~/.vkx/lib/<平台>/` 的 imported target |
| **只有头文件** | cpp-httplib、stb_image、GLM | 一条 `target_include_directories` |
| **源码**（C++ ABI，不能预编译） | Jolt、GameNetworkingSockets、Tracy | `add_subdirectory` 进 `~/.vkx/src/<库>` |

C++ 库不发二进制，因为它的 `.a` 要和你的标准库实现、异常/RTTI 开关、Windows
运行时全部对齐，对不上就是链接失败或者运行时崩在 `std::string` 的析构里。

---

## 命令

```sh
vkx add jolt          # 把 [libs] 里的 jolt 改成 true，重新生成 CMake
vkx remove jolt
vkx deps              # 列出 sdk 包里有什么、哪些正在参与构建
```

不追版本：vkx 的版本号就是依赖集的版本号，一个 vkx 对应一套确定的库，
不存在版本求解和锁文件。

`vkx add` 做三件事：

1. 查镜像清单，确认这个库有、版本对
2. 该库的源码或二进制不在 `~/.vkx` 里就下载过来（增量，不用重装整套环境）
3. 写 `vkx.toml`，重新生成 `target/CMakeLists.txt`

`build` / `run` / `dist` 在开跑之前都会检查一次：`vkx.toml` 的修改时间比生成的
CMakeLists 新，或者 `[vkx] version` 和当前 vkx 对不上，就先重新生成。

---

## 生成的 CMakeLists 骨架

生成物是给人看的——出问题时用户读的是他没写过的文件，所以要带注释、排版正常。

```cmake
# 由 vkx 0.2.0 从 ../vkx.toml 生成。别手改这个文件，改了下次构建会被覆盖。
# 要加 TOML 表达不了的东西，写进 [build] cmake_include 指向的那个文件。
cmake_minimum_required(VERSION 3.24)
project(mygame LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ---- 源码：来自 [source] ----
file(GLOB_RECURSE VKX_SOURCES CONFIGURE_DEPENDS
     ${CMAKE_SOURCE_DIR}/../src/*.cpp ...)

add_executable(mygame ${VKX_SOURCES})          # Android 上是 add_library(main SHARED ...)
target_include_directories(mygame PRIVATE ../src)

# ---- 依赖：来自 [dependencies] ----
# SDL3  预编译
add_library(SDL3::SDL3 STATIC IMPORTED)
set_target_properties(SDL3::SDL3 PROPERTIES
    IMPORTED_LOCATION "$ENV{HOME}/.vkx/lib/macos-arm64/libSDL3.a" ...)

# cpp-httplib  仅头文件
target_include_directories(mygame PRIVATE "$ENV{HOME}/.vkx/include/cpp-httplib")

# Jolt  源码（[libs] jolt = true 时才有这段）
add_subdirectory("$ENV{HOME}/.vkx/src/jolt" jolt EXCLUDE_FROM_ALL)
target_link_libraries(mygame PRIVATE Jolt)

# Tracy  源码（开发期用，出货不带）
if(VKX_TARGET_DESKTOP)
    add_subdirectory("$ENV{HOME}/.vkx/src/tracy" tracy EXCLUDE_FROM_ALL)
    target_link_libraries(mygame PRIVATE TracyClient)
endif()

# ---- 着色器：来自 [shaders] ----
# ...每个入口一条 slangc 规则 + 转 C 数组

# ---- 逃生舱 ----
include(${CMAKE_SOURCE_DIR}/../extra.cmake OPTIONAL)
```

---

## IDE

根目录生成 `CMakePresets.json`，指向 `target/`。CLion、Visual Studio、
VS Code 的 CMake 扩展都读它，打开工程目录就能直接构建调试，不用手动指路。

iOS 的 `.xcodeproj` 仍然由 CMake 生成到 `target/ios/`，和现在一样。

---

## 对第一章讲稿的影响

第九步现在有两个 CMake diff——源码清单按三层重排、`target_include_directories(src)`。
两个都会消失：源码自动收集，`src/` 默认就在搜索路径里。读者只需要把文件挪进
`gpu/` 和 `ui/`，然后重新构建。

其余八步不涉及 CMake，不受影响。
