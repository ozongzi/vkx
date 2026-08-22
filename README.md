# vkx

Vulkan + SDL3 跨平台游戏工程脚手架。一条命令建工程，一条命令跑起来，
Windows / macOS / Linux / Android / iOS 同一套代码。

```sh
vkx new mygame
cd mygame
vkx run
```

屏幕上出现一个三角形，就说明环境齐了。

## 分工

| 谁 | 干什么 |
| --- | --- |
| **离线安装包** | 一个开发平台一个自解压程序，里面是 vkx 二进制加它要的全部依赖（上游原包，没拆） |
| **安装包自己** | 双击或者命令行跑一下，它把里面的东西校验、解开、摆进 `~/.vkx`。解压、校验全在进程内，不依赖机器上有 unzip / tar / sha256sum |

**vkx 不联网。** 它要的一切都在安装包里，依赖清单和每一样的 blake3 都编在二进制里。
读者的机器上只会多出一个 `~/.vkx` 目录，删掉就等于卸载干净。

## 安装

从 [Release 页面](https://github.com/ozongzi/vkx/releases/latest)下对应你机器的那一个，
跑一下就装好了——它自己就是安装程序，不需要先解压、也不需要再执行别的命令。

| 你的机器 | 下这个 | 大小 |
| --- | --- | --- |
| Windows（Intel/AMD 64 位） | `vkx-setup-windows-x64.exe` | 1.76 GiB |
| Linux（Intel/AMD 64 位） | `vkx-setup-linux-x64` | 1.72 GiB |
| macOS（Apple 芯片） | `vkx-setup-macos-arm64` | 1.94 GiB |

```sh
chmod +x vkx-setup-macos-arm64
./vkx-setup-macos-arm64
```

装完 vkx 就在 `~/.vkx/bin/vkx`，并且已经写进 PATH。

第一次运行时系统会拦一下，因为这个包没有 Apple / 微软的开发者签名：

- **macOS** 弹「Apple 无法验证……是否包含恶意软件」。系统设置 → 隐私与安全性，
  往下翻到「已阻止使用」点「仍要打开」；或者摘掉隔离标记：
  `xattr -d com.apple.quarantine vkx-setup-macos-arm64`
- **Windows** SmartScreen 会警告但不拦，点「更多信息 → 仍要运行」。

装的过程只补缺的：已经装好并且校验通过的直接跳过，所以重复执行是安全的，
中断之后再跑一次就接着装。每一样在装之前都按 blake3 校验，对不上就不装、
不留半成品。

`vkx doctor` 列出哪些装了、哪些没装。

### 只有两样东西装不进 ~/.vkx

| 缺什么 | 为什么 | 脚本会怎么做 |
| --- | --- | --- |
| macOS 的 Xcode / 命令行工具 | Apple 的 SDK 不允许第三方再分发，iOS 构建也必须用 `xcodebuild` | 提示执行 `xcode-select --install` |
| 显卡驱动里的 Vulkan ICD | Windows 的 `vulkan-1.dll`、Linux 的 ICD 都由驱动提供 | 提示更新驱动或装 `mesa-vulkan-drivers` |

其余的（C++ 编译器、CMake、Vulkan 相关的库和工具）都是可重定位的二进制，
一律装进 `~/.vkx`。Windows 上统一用 llvm-mingw，不需要 Visual Studio——
就算机器上装了也不用它：SDK 包里的预编译库是 llvm-mingw 编的，
两种 ABI 混在一起会在链接期炸，而 MSVC 的工具集也不允许我们分发。

## 命令

| 命令 | 作用 |
| --- | --- |
| `vkx new [名字] [--package-id <包名>]` | 生成工程，顺带生成 Android release 签名密钥 |
| `vkx build [--release] [--target <平台>]` | 构建 |
| `vkx run [--release] [--target <平台>]` | 构建并运行 |
| `vkx dist [--target <平台>]` | 打出可以直接分发的安装包 |
| `vkx fmt [--check]` | 按工程根的 `.clang-format` 格式化 `src/`；`--check` 只检查不改，给 CI 用 |
| `vkx clean` | 删掉 build/ |

`--target` 可选：`desktop`（默认）、`android`、`ios`（模拟器）、`ios-device`。

工程名和包名都是必填的。命令行上没写的会在终端里逐个问，回车即接受默认值：

```
$ vkx new
  工程名 (mygame) › mygame
  包名 (Android / iOS) (com.example.mygame) › com.moba.mygame
```

非交互环境（管道、CI）不会卡住等输入，而是直接报错，要求把两个参数写全。

## 环境布局

安装包把里面的东西摆成这个样子：

```
~/.vkx/
  bin/vkx
  sdk/.installed/<组件>          装好的戳，内容是那个组件的 blake3
  sdk/toolchain/{cmake,ninja,slang,clang-format}
  sdk/vulkan/{vulkan,moltenvk}
  sdk/toolchain/{llvm-mingw,llvm} Windows 和 Linux 自带的 C++ 编译器
  sdk/libs/<target>/             预编译库，一个 target 一份
  sdk/android/{jdk,gradle,sdk}   JDK / Gradle / Android SDK / NDK
  sdk/maven/                     安卓构建要的 Gradle 依赖（AGP 及其闭包）
  sdk/sdl3-android/              SDL3 的安卓 .aar
```

`sdk/` 下一个目录对应依赖表里的一个条目，解包就是按这个对应关系走的。
判定「已安装」= 目录在 + 戳在 + 戳的值和二进制里硬编码的一致。校验的是安装包里
那个文件的 blake3，不是装完那棵树——树哈希每跑一次命令都要过 2.8 GB 的 NDK，
不现实。`VKX_HOME` 可以整体换个位置。

`sdk/vulkan/vulkan` 里是 Vulkan loader 和 khronos 校验层。校验层不在显卡驱动里，
Debug 构建要靠它报出用错 Vulkan 的地方，所以得自己分发。上游只有 LunarG 的整包
（每平台 274~493 MB，macOS / Windows 版还是安装脚本跑不动的 Qt 安装器），
于是当初挑出需要的几个文件重打包成几十 MB，现在这份就在离线安装包里。

### 运行期的环境变量

`vkx run` 启动程序前会补几个变量，指向 SDK 里的 Vulkan（见
`toolchain::vulkan_runtime_env`）。macOS 上设 `SDL_VULKAN_LIBRARY`（loader 的
绝对路径）和 `VK_DRIVER_FILES`：那是唯一连 loader 都没有的平台，不明确指定的话
SDL 会退而直接加载 MoltenVK，绕过 loader，校验层就无从插入——症状是程序照常
运行，只多一行「校验层不可用」。

**不能用 `DYLD_LIBRARY_PATH`。** macOS 的 SIP 会在执行受保护的系统二进制时把
`DYLD_*` 剥掉，中间隔一层 `/usr/bin/env` 或 `/bin/sh` 就没了。

Linux 和 Windows 的 loader 由显卡驱动提供，只补校验层的目录。

## 分发

`vkx dist` 出的是能直接发给别人的东西，全部落在工程的 `dist/` 下：

| 平台 | 产物 | 说明 |
| --- | --- | --- |
| macOS | `.app` + `.dmg` | MoltenVK 放进 `Contents/Frameworks/`——SDL 的默认搜索路径第一项就是这里，所以不需要任何额外代码或环境变量；ad-hoc 签名（上架或免右键打开还需公证） |
| Windows | `.zip` | 可执行文件静态链接，解压即用 |
| Linux | `.tar.gz` | 同上 |
| Android | 签名 `.apk` + `.aab` | APK 直接安装，AAB 上架 Google Play |
| iOS | —— | 不由 vkx 出。`vkx build --target ios-device` 生成 Xcode 工程，Archive 在 Xcode 里做 |

## 移动端开箱即用

**Android 签名**：`vkx new` 时就用 keytool 生成 `android/keystore/release.jks`
和随机口令，写进 `android/keystore.properties`（两者都在 .gitignore 里）。
所以 `vkx build --release --target android` 直接产出签名好的 APK。
上架商店请换成你自己保管的正式密钥。

**Xcode 工程**：`vkx build --target ios-device` 生成 `.xcodeproj` 就停手
（路径会打印出来）。用 Xcode 打开，在里面选 Team 打开自动签名，就能连真机跑、
也能 Archive 上架。

vkx 不代劳签名和出 `.ipa`：那些事绑 Apple 账号（证书、描述文件、团队 ID），
`exportOptions.plist` 的格式还跟着 Xcode 版本变。vkx 发出去之后不再更新，
代劳只会留下一个修不好的报错。模拟器那条内循环不受影响：`vkx run --target ios`
照旧编译、安装、启动。

## 开发

```sh
cargo build
cargo run -- new /tmp/demo
```

工程模版在 `template/`，用 `include_dir!` 在编译期整个嵌进二进制。
`build.rs` 声明了对该目录的依赖，改完模版直接 `cargo build` 就会重新嵌入。

发版：打一个 `v*` 标签，`.github/workflows/release.yml` 为三个开发平台交叉编译
出裸二进制，上传到 GitHub Release。CI 只做这一件事——不打包、不校验。依赖随
离线安装包走，和这条流水线无关。

依赖表在 `src/sdk.rs`：一个 vkx 版本对应一套确定的依赖，版本钉死，出 CVE 也不动。
换依赖就发新版 vkx。

## 平台验证状态

| 平台 | 状态 |
| --- | --- |
| macOS (Apple Silicon) | 已验证：像素回读确认三角形；`.app` 在干净环境下用包内 MoltenVK 正常启动 |
| iOS（模拟器 / 真机） | 已验证：构建 → 安装 → 启动。真机和上架走 Xcode，vkx 只生成工程 |
| Android (arm64 / x64) | 已验证：签名 APK（apksigner 校验通过）+ AAB 产出，真机和模拟器都跑过 |
| Windows (x64) | 已验证：安装、建工程、构建、运行 |
| Linux (x64) | 已验证：安装、建工程、构建、运行 |
