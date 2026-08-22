# vkx

用 C++ 写 Vulkan 游戏的脚手架。一条命令建工程，一条命令跑起来，
Windows / macOS / Linux / Android / iOS 同一套代码。

```sh
vkx new mygame
cd mygame
vkx run
```

屏幕上出现一个三角形，就说明环境齐了。这中间没让你装 CMake、装 Vulkan SDK、
装 NDK、配环境变量——那些 vkx 都带着。

## 装 vkx

从 [Release 页面](https://github.com/ozongzi/vkx/releases/latest)下对应你机器的
那一个，跑一下就装好了。它自己就是安装程序，不用先解压、也不用再执行别的命令。

| 你的机器 | 下这个 | 大小 |
| --- | --- | --- |
| Windows（Intel/AMD 64 位） | `vkx-setup-windows-x64.exe` | 1.76 GiB |
| Linux（Intel/AMD 64 位） | `vkx-setup-linux-x64` | 1.72 GiB |
| macOS（Apple 芯片） | `vkx-setup-macos-arm64` | 1.94 GiB |

```sh
chmod +x vkx-setup-macos-arm64
./vkx-setup-macos-arm64
```

包大是因为里面装着编译器、CMake、Ninja、着色器编译器、Vulkan 一整套、
Android 的 JDK 和 NDK，以及五个平台的预编译库。东西全在 `~/.vkx` 里，
`vkx self uninstall` 删干净。

**第一次运行系统会拦一下**，因为这个包没有 Apple / 微软的开发者签名：

- **macOS** 弹「Apple 无法验证……是否包含恶意软件」。系统设置 → 隐私与安全性，
  往下翻到「已阻止使用」点「仍要打开」；或者摘掉隔离标记：
  `xattr -d com.apple.quarantine vkx-setup-macos-arm64`
- **Windows** SmartScreen 会警告但不拦，点「更多信息 → 仍要运行」

有两样东西 vkx 给不了，得你自己装：

| 缺什么 | 什么时候要 | 怎么办 |
| --- | --- | --- |
| Xcode 命令行工具 | 在 macOS 上编译任何东西 | `xcode-select --install` |
| 显卡驱动里的 Vulkan | Windows / Linux 上运行 | 更新显卡驱动；Linux 上装 `mesa-vulkan-drivers` |

不确定齐没齐就跑 `vkx doctor`，它逐项列出来。

## 建一个工程

```sh
vkx new mygame
```

不写参数就在终端里逐个问，回车接受默认值：

```
$ vkx new
  工程名 (mygame) › mygame
  包名 (Android / iOS) (com.example.mygame) › com.moba.mygame
```

写脚本或者在 CI 里跑要写全，它不会卡住等输入：

```sh
vkx new mygame --package-id com.moba.mygame
```

加 `--server` 建服务端工程：一个 HTTP 服务，没有窗口也没有渲染，
依赖里也就没有 SDL 和 Vulkan。

生成的工程长这样：

```
mygame/
  vkx.toml          唯一需要你编辑的配置
  src/              你的代码。默认放着一份能跑的 Vulkan 三角形
  shaders/          着色器（Slang）
  android/  ios/  macos/   各平台的工程壳，一般不用动
  .clang-format     vkx fmt 用的格式规则
  CMakePresets.json 生成的，VS Code / CLion 打开工程目录就能认出构建配置
  target/           构建产物，构建后才出现，整个不进版本库
  dist/             vkx dist 的产出，同样是生成的
```

`android/keystore/release.jks` 是 `vkx new` 顺手生成的 Android 签名密钥
（口令在 `android/keystore.properties`，两个都在 `.gitignore` 里）。所以你一上来
就能出签名好的 APK。真要上架商店，换成你自己保管的正式密钥。

## 日常

```sh
vkx build              # 编译
vkx run                # 编译并运行
vkx run -- --level 3   # -- 之后的参数原样传给你的程序（桌面）
vkx fmt                # 按 .clang-format 格式化 src/
vkx clean              # 删掉构建产物
```

加 `--release` 出优化过的版本。加 `--target` 换平台：

| `--target` | 干什么 |
| --- | --- |
| `desktop`（默认） | 本机跑 |
| `android` | 编译、装到连着的设备或模拟器、启动 |
| `ios` | 编译、装进 iOS 模拟器、启动 |
| `ios-device` | 生成 Xcode 工程就停手，签名和真机调试在 Xcode 里做 |

## 加一个库

最常做的一件事，所以单独说。

```sh
vkx deps          # 看有哪些
vkx add SQLite    # 加一个
vkx remove SQLite # 去掉
```

```
==> 依赖（vkx.toml 的 dependencies）
    ● SDL3                   窗口、输入、音频、文件对话框
    ● Vulkan                 Vulkan 头文件和 volk 函数指针加载
    ○ zlib                   压缩
    ○ FreeType               字体栅格化
    ○ mbedTLS                轻量 TLS
    ○ OpenSSL                加密
    ○ protobuf               序列化
    ○ GameNetworkingSockets  局内实时传输（UDP、加密、P2P 打洞）
    ○ Jolt                   物理引擎
    ○ GLM                    向量和矩阵（纯头文件）
    ○ cpp-httplib            HTTP 客户端和服务端（纯头文件）
    ○ SQLite                 嵌入式 SQL 数据库（单文件、无服务进程）
    ○ stb                    PNG / JPEG 编解码（纯头文件）

    ● 已启用   ○ 未启用
```

加完直接 `#include` 就行，头文件路径和链接 vkx 都安排好了：

```cpp
#include <sqlite3.h>
#include <httplib.h>
#include <glm/glm.hpp>
```

**这些库全是预编译好的**，随安装包一起装在你机器上了。所以 `vkx add`
不下载、不编译——改这个列表不影响构建时间，只决定链哪些库、暴露哪些头文件。
不用为「以后可能要用」提前加，用到再加，一秒钟的事。

依赖之间的关系 vkx 自己补。`vkx add GameNetworkingSockets` 之后 `vkx.toml` 里
只多了这一行，但构建时 zlib、OpenSSL、protobuf 会一起链进来——你不用去查它要什么。

五个平台的库都是同一套源码、同一套工具链编的，所以在 macOS 上 `vkx add Jolt`，
`--target android` 照样能编。

## 加代码和着色器

**源文件**：往 `src/` 里放 `.cpp` / `.h`，递归收集，不用改任何配置。

**着色器**：往 `shaders/` 里放 `.slang`。带 `[shader("vertex")]`、
`[shader("fragment")]` 标注的入口点会被自动找出来，编成 SPIR-V，再转成头文件
嵌进可执行文件——所以运行时不用带着 `.spv` 到处跑。

写在 `shaders/rect.slang` 里：

```slang
[shader("vertex")]
VertexOutput vertex_main(...) { ... }
```

C++ 这边就能直接用：

```cpp
#include "rect_vert.spv.h"   // 里面是 RECT_VERT_SPV 和 RECT_VERT_SPV_SIZE
```

加一个入口点自动多一条规则，同样不用改配置。

## vkx.toml

工程根目录下唯一需要你编辑的文件：

```toml
[project]
name = "mygame"                    # 可执行文件名
package_id = "com.moba.mygame"     # Android 的 applicationId / iOS 的 bundle id
version = "0.1.0"                  # dist 的包名和各平台版本号都取它
dependencies = ["SDL3", "Vulkan"]  # vkx add / remove 改的就是这一行

[vkx]
version = "0.4.1"                  # 生成这个工程时用的 vkx 版本
```

`CMakeLists.txt` 由 vkx 生成到 `target/`，**别手改**，每次构建都会覆盖。
它表达不了的东西写进工程根目录的 `extra.cmake`，会在末尾被 `include`。

## 发布

```sh
vkx dist                    # 本机桌面
vkx dist --target android
```

产物全部落在工程的 `dist/` 下：

| 平台 | 产物 | 说明 |
| --- | --- | --- |
| macOS | `.app` + `.dmg` | MoltenVK 打包在 `.app` 里，对方机器上什么都不用装 |
| Windows | `.zip` | 静态链接，解压即用 |
| Linux | `.tar.gz` | 同上 |
| Android | 签名 `.apk` + `.aab` | APK 直接装，AAB 上架 Google Play |
| iOS | —— | 见下 |

**iOS 不由 vkx 打包。** `vkx build --target ios-device` 生成 `.xcodeproj`
就停手，用 Xcode 打开，选 Team 打开自动签名，就能连真机跑、也能 Archive 上架。

不代劳是有意的：签名绑着 Apple 账号（证书、描述文件、团队 ID），
`exportOptions.plist` 的格式还跟着 Xcode 版本变。vkx 发出去之后不再更新，
代劳只会给你留一个修不好的报错。模拟器那条内循环不受影响，
`vkx run --target ios` 照旧编译、安装、启动。

## 出问题的时候

```sh
vkx doctor       # 环境齐不齐，缺什么怎么补
vkx help         # 列出所有专题和错误码
vkx help E0003   # 展开某个错误码
vkx help ios     # 展开某个专题
```

报错都带一个 `E00xx` 编号，`vkx help E00xx` 有完整解释和处理办法。
专题目前有 `manifest`、`toolchain`、`ios`、`install`、`version`。

## 几个「为什么」

**为什么不联网。** vkx 要的一切都在安装包里，依赖清单和每一样的校验值都编在
二进制里。装完之后建工程、构建、运行、打包全程不出网——所以三年后照样能跑，
不会因为某个上游改了地址就构建不动。

**为什么依赖不追最新版。** 一个 vkx 版本对应一套确定的依赖，版本钉死，出了
CVE 也不动，要换就发新版 vkx。这样同一个 vkx 版本在任何人的机器上、任何时间
编出来的东西都一样。

**为什么装在 `~/.vkx`。** 不碰系统目录，不要管理员权限，删掉那个目录就等于
卸载干净。想装到别处设 `VKX_HOME`。

**为什么 Windows 上不用 Visual Studio。** 那边用的是 llvm-mingw。三个桌面平台
用的都是 clang，语言扩展、警告、ABI 的脾气一致，少一整套要维护的差异——
代价是你得用 mingw 那套而不是 MSVC。

## 平台验证状态

| 平台 | 状态 |
| --- | --- |
| macOS (Apple Silicon) | 已验证：像素回读确认三角形；`.app` 在干净环境下用包内 MoltenVK 正常启动 |
| iOS（模拟器 / 真机） | 已验证：构建 → 安装 → 启动 |
| Android (arm64 / x64) | 已验证：签名 APK（apksigner 校验通过）+ AAB 产出，真机和模拟器都跑过 |
| Windows (x64) | 已验证：安装、建工程、构建、运行 |
| Linux (x64) | 已验证：安装、建工程、构建、运行 |

## 自己改 vkx

```sh
cargo build
cargo run -- new /tmp/demo
```

工程模版在 `template/`，编译期整个嵌进二进制，改完 `cargo build` 就重新嵌入。
依赖表在 `src/sdk.rs`。发版打一个 `v*` 标签，CI 交叉编译出三个平台的裸二进制；
那三个 GB 级的离线安装包是另外拼的。
