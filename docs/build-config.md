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

这份文档原来列的是一份设计草案，字段比实际实现的多。下面是**当前真的会被读取**
的全部字段——多写的会被忽略，不会报错，所以这里如实列清楚：

```toml
[project]
name = "mygame"                  # 可执行文件名，同时是 CMake target 名
package_id = "com.moba.mygame"   # Android applicationId / iOS bundle id
version = "0.1.0"                # dist 的包名和各平台版本字段都取它
dependencies = ["SDL3", "Vulkan"]  # 要链哪些库、暴露哪些头文件

[vkx]
version = "0.2.7"                # 生成这个工程时用的 vkx 版本
```

草案里有过、但**没有实现**的：`cxx_standard`（固定 C++20）、`[source]` 的
`dirs` / `exclude`（固定递归收集 `src/` 下的 `.c .cc .cpp .cxx`）、`[shaders]`
的各项（固定扫 `shaders/*.slang`）、`[embed]`、`[build] cmake_include`
（逃生舱是工程根目录的 `extra.cmake`）。想要哪个再加，但别在文档里写着
却不生效——那比没有更糟。

源码和着色器都是自动收集的：往 `src/` 里加文件、往 `shaders/` 里加一个
`[shader(...)]` 入口点，都不用改任何配置。

---

## 依赖：全部预编译

| 形态 | 例子 | 生成的 CMake |
| --- | --- | --- |
| **有 CMake 配置包** | SDL3、FreeType、mbedTLS、protobuf、GNS、Jolt、OpenSSL | `find_package(<包> REQUIRED CONFIG)` + `target_link_libraries` |
| **只有 Find 模块** | zlib | `find_package(ZLIB REQUIRED)`（模块模式，zlib 不发配置包） |
| **只有头文件** | GLM、cpp-httplib、stb | 什么都不生成——`include/` 整个已经在搜索路径上 |
| **自己编的一个 .c** | volk | `add_library(volk STATIC ...)`，见下 |

早先这里写的是「C++ 库不能预编译，只能 `add_subdirectory` 源码现编」。那个判断
的前提是编译器不确定；现在工具链钉死了（Windows llvm-mingw、Linux 自带的
clang + 静态 libc++、Apple 平台 Xcode 的 clang），C++ 库就可以也应该预编译——
每个 target 一份，见 `vkx.md`。

volk 是例外：它就一个 `.c`，我们自己编。不用它自带的 CMake 包，是因为那个包会
把打包机器的绝对路径写进 `INTERFACE_INCLUDE_DIRECTORIES`，在读者机器上不存在。

### 传递依赖由 vkx 补齐

各家的配置包把自己用到的库写进 link interface，却不替你 `find_package`。所以
只声明 FreeType 会报「target ZLIB::ZLIB not found」——那个错离真正的原因隔着
一层。依赖表里记了 `requires`：

```
FreeType              -> zlib
protobuf              -> zlib
GameNetworkingSockets -> protobuf, OpenSSL
```

展开之后按表的顺序排，被依赖的先 `find_package`。

平台差异也在表里：OpenSSL 只有非 Windows 编了（Windows 上 GNS 用系统的
BCrypt），它的 `find_package` 包在 `if(NOT WIN32)` 里。

---

## 命令

```sh
vkx deps                  # 列出全部可用的依赖，标出哪些已启用
vkx add Jolt              # 往 dependencies 里加一个
vkx remove FreeType
```

因为库全是预编译好的，开关**不影响构建时间**——它只决定链哪些库、暴露哪些
头文件。以前那套「打开一个就多等几分钟编译」已经没有了。

`vkx add` 就地改 `vkx.toml` 里的 dependencies 数组，保留你的注释和字段顺序
（不是整个文件重新序列化），并按依赖表的顺序重排。

不追版本：vkx 的版本号就是依赖集的版本号，一个 vkx 对应一套确定的库，
不存在版本求解和锁文件。

---

## 生成的 CMakeLists 骨架

生成物是给人看的——出问题时用户读的是他没写过的文件，所以要带注释、排版正常。

真实产物长这样（`vkx new` 之后 `vkx build` 一次，看 `target/CMakeLists.txt`）：

```cmake
# 由 vkx 0.2.7 从 ../vkx.toml 生成。别手改这个文件——下次构建会覆盖掉。

cmake_minimum_required(VERSION 3.24)
project(mygame LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 20)

# ---- 预编译库：一个 target 一份，目标在配置期才定得下来 ----
set(VKX_SDK_LIBS_ROOT "~/.vkx/sdk/libs" CACHE PATH "")
if(ANDROID)
    if(ANDROID_ABI STREQUAL "arm64-v8a")
        set(VKX_SDK_LIBS "${VKX_SDK_LIBS_ROOT}/android-arm64")
    else()
        set(VKX_SDK_LIBS "${VKX_SDK_LIBS_ROOT}/android-x64")
    endif()
elseif(IOS)
    set(VKX_SDK_LIBS "${VKX_SDK_LIBS_ROOT}/ios-arm64")
else()
    set(VKX_SDK_LIBS "${VKX_SDK_LIBS_ROOT}/macos-arm64")
endif()
list(APPEND CMAKE_PREFIX_PATH "${VKX_SDK_LIBS}")

# ---- SDL3 和 Vulkan：声明了才生成，服务端工程这一整段都没有 ----
find_package(VulkanHeaders REQUIRED CONFIG)
if(NOT IOS)
    add_library(volk STATIC "${VKX_SDK_LIBS}/include/volk.c")
    ...
endif()

# ---- 目标：源码递归收集自 src/ ----
file(GLOB_RECURSE VKX_SOURCES CONFIGURE_DEPENDS "${VKX_ROOT}/src/*.cpp" ...)
add_executable(mygame ${VKX_SOURCES})
target_include_directories(mygame PRIVATE "${VKX_ROOT}/src")

# ---- 依赖：来自 vkx.toml 的 dependencies ----
# 窗口、输入、音频、文件对话框
find_package(SDL3 REQUIRED CONFIG)
target_link_libraries(mygame PRIVATE SDL3::SDL3)

# 字体栅格化
find_package(Freetype REQUIRED CONFIG)
target_link_libraries(mygame PRIVATE Freetype::Freetype)

# ---- 着色器：扫 shaders/*.slang 的 [shader(...)] 入口自动生成 ----
vkx_add_slang_shader(mygame SOURCE ... ENTRY vertex_main STAGE vertex ...)

# ---- 逃生舱 ----
include("${VKX_ROOT}/extra.cmake" OPTIONAL)
```

里面没有 `FetchContent`，一处都没有——库全在包里，构建期不出网。

---

## IDE

根目录生成 `CMakePresets.json`，指向 `target/`。CLion、Visual Studio、
VS Code 的 CMake 扩展都读它，打开工程目录就能直接构建调试，不用手动指路。

iOS 的 `.xcodeproj` 由 CMake 生成到 `build/ios/`（真机）或 `build/ios-simulator/`。
`vkx build --target ios-device` 生成完就停手，签名和上架在 Xcode 里做。

---

## 对第一章讲稿的影响

第九步现在有两个 CMake diff——源码清单按三层重排、`target_include_directories(src)`。
两个都会消失：源码自动收集，`src/` 默认就在搜索路径里。读者只需要把文件挪进
`gpu/` 和 `ui/`，然后重新构建。

其余八步不涉及 CMake，不受影响。
