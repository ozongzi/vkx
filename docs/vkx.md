# vkx 是什么

一个 Vulkan + SDL3 的跨平台游戏工程脚手架。它管三件事：把工具链装齐、把工程
构建出来、把产物打成能发给别人的包。

设计上只有一条主线：**装完之后，构建期不出网。**

---

## 一切都在离线安装包里

一个开发平台一个包，里面是这个平台需要的全部东西：

| | |
|---|---|
| 工具 | cmake、ninja、slangc、clang-format |
| C++ 编译器 | Windows 的 llvm-mingw、Linux 的 LLVM（macOS 用 Xcode 的） |
| 预编译库 | 每个 target 一份，见下 |
| Vulkan | loader、校验层，macOS 上还有 MoltenVK |
| 安卓 | JDK、Gradle、SDK、NDK、SDL3 的 .aar、Gradle 的依赖仓库 |

```sh
./vkx-setup-macos-arm64        # 双击或在终端里跑，半分钟
```

自解压包 = 安装器 + 一个 zip 首尾相接（见 `payload.rs`）。安装器只取出 vkx
本身，剩下的交给 `vkx install <安装包路径>` ——安装包自己就是那个 zip，所以
不必先解出来再喂进去，省掉一次 1.6 GB 的临时拷贝。

每一样在装之前按 blake3 校验，对不上就不装、不留半成品。装到一半断了再跑一次
就接着装：已经装好并且校验通过的直接跳过。

### 不调任何外部二进制

下载、解压、校验全在进程内：解压用纯 Rust 的 tar + flate2 + ruzstd，校验用
blake3。不用 curl / tar / sha256sum，是因为它们靠不住——Windows 上
`sha256sum` 根本不存在，System32 里那个 bsdtar 是 2017 年的 libarchive，
不支持 zstd。依赖「机器上碰巧装了什么」的工具，出问题很难远程判断。

---

## 预编译库：一个 target 一份

```
~/.vkx/sdk/libs/
  macos-arm64/    linux-x64/    windows-x64/
  android-arm64/  android-x64/  ios-arm64/
```

每份里是同一套库的该平台版本：SDL3、FreeType、mbedTLS、zlib、Vulkan-Headers、
volk、OpenSSL、protobuf、GameNetworkingSockets、Jolt，外加纯头文件的 GLM、
cpp-httplib、stb。

每个包只带自己用得上的那几份：macOS 的包带桌面 + 安卓两个 ABI + iOS，
Linux 和 Windows 的包带自己 + 安卓两个 ABI（安卓靠 NDK，三个平台都能编；
iOS 要 Xcode，只有 macOS 能编）。

**全部从源码编，不抓各家的官方二进制。** C 的 ABI 是稳的，混着用还看不出问题；
C++ 不是。vkx 把工具链钉死了，官方二进制多半是别的编译器、别的标准库编的，
混进来会在链接期炸——或者更糟，链上了但运行时行为不对。

**没有联网兜底。** 早先的版本在 `find_package` 失败时会用 FetchContent 去
GitHub 拉一份源码现编。那看着贴心，实际是个会静默生效的不可复现来源：上游的
tag 可能移动，学员和作者的产物可能不是同一个东西。现在全删了，找不到就报错。

---

## 依赖开关

`vkx.toml` 里一个数组：

```toml
[project]
dependencies = ["SDL3", "Vulkan", "FreeType"]
```

```sh
vkx deps                  # 列出全部可用的，标出哪些已启用
vkx add Jolt
vkx remove FreeType
```

因为库全是预编译好的，这个开关**不影响构建时间**，只决定链哪些库、暴露哪些
头文件。传递依赖由 vkx 补齐——声明 FreeType 就自动带上 zlib，声明
GameNetworkingSockets 就自动带上 protobuf 和 OpenSSL。各家的 CMake 配置包会
把用到的库写进 link interface 却不 `find_package`，少一个就报
「target ZLIB::ZLIB not found」，那个错离真正原因隔着一层。

---

## 构建

```sh
vkx build [--release] [--target desktop|android|ios|ios-device]
vkx run   [--release] [-- 传给游戏的参数]
```

读 `vkx.toml`，生成 `target/CMakeLists.txt`，调 cmake + ninja。生成的文件
每次构建都会覆盖，不要手改；它表达不了的东西写进工程根目录的 `extra.cmake`，
会在末尾被 include。

C++ 编译器一律是 clang：Windows 用包里的 llvm-mingw（不能用 MSVC——预编译库
是 llvm-mingw 编的，两种 ABI 混在一起会在链接期炸），Linux 用包里的 LLVM 并
静态链 libc++（发行版默认往往连 g++ 都没有，而且 Linux 上的 clang 默认仍用
系统的 libstdc++，各机器不一样），macOS 和 iOS 用 Xcode 的 Apple clang。

---

## 打包

```sh
vkx dist [--target desktop|android]
```

| 平台 | 产物 |
|---|---|
| macOS | `.app`（内嵌 MoltenVK，ad-hoc 签名）+ `.dmg` |
| Windows | `.zip`（静态链接，解压即用） |
| Linux | `.tar.gz` |
| Android | 签名 APK + AAB |

**iOS 不在其中。** vkx 到「生成 Xcode 工程」为止：

```sh
vkx build --target ios-device     # 生成 .xcodeproj，然后交给 Xcode
```

签名绑 Apple 账号（证书、描述文件、团队 ID），`exportOptions.plist` 的格式还
跟着 Xcode 版本变。vkx 发出去之后不再更新，代劳只会给学员留下一个修不好的报错。

---

## 两套模版

```sh
vkx new game            # 客户端：窗口、Vulkan 渲染、五个平台的壳
vkx new api --server    # 服务端：一个 HTTP 服务，没有窗口也没有渲染
```

服务端不声明 SDL3 和 Vulkan，所以生成的 CMakeLists 里一行相关的东西都没有，
产物也不依赖图形栈。

---

## help 要能当手册用

```sh
vkx help              # 命令一览
vkx help manifest     # vkx.toml 有哪些字段
vkx help toolchain    # 工具链装在哪、怎么清
vkx help install      # 工具链是怎么装的
vkx help ios          # 从零到真机的完整路径
vkx help version      # 为什么依赖不追版本
vkx help E0003        # 展开某个错误码
```

---

## 每个错误都要给出解法

错误类型自带 `hint` 字段，构造错误时必须填。**没有解法的错误编译不过**——这是
类型系统层面的约束，不是纪律。

| 码 | 含义 |
|---|---|
| E0001 | 不在 vkx 工程里（往上都没找到 vkx.toml） |
| E0002 | vkx.toml 读不出来，或缺必填字段 |
| E0003 | 工具链的某个组件不在 ~/.vkx 里 |
| E0004 | 调用的外部命令失败（cmake / ninja / slangc / gradle / xcodebuild） |
| E0005 | 读写文件失败 |
| E0006 | 当前环境不满足这条命令的要求 |
| E0007 | 命令用法不对 |

IO 错误必须带上下文：不能只说「文件不存在」，要说是在做什么的时候、哪个文件。
