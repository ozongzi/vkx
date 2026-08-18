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
| **安装脚本** | 只把 vkx 二进制放进 `~/.vkx/bin` 并接进 PATH，二三十行 |
| **vkx** | 按需下载工具链，用 HTTP Range 只取 SDK 包里需要的那一段。下载、解压、校验全在进程内，不依赖机器上有 curl / tar / sha256sum |
| **镜像** | 你自己的服务器。`mirror/` 下的脚本编出各平台的 SDK 包，只要静态文件服务支持 Range 就能对外提供 |

这样读者的机器上只会多出一个 `~/.vkx` 目录，删掉就等于卸载干净；
构建过程也完全不碰 GitHub，国内网络下不会卡在下载上。

## 安装

macOS / Linux：

```sh
curl -fsSL https://yinli.tech/file/install.sh | sh
```

Windows：

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force; iwr -useb https://yinli.tech/file/install.ps1 | iex
```

脚本只装 vkx 自己（几 MB）。工具链由 vkx 按需下载：第一次 `vkx build` 取桌面
需要的那几个组件，第一次 `vkx build --target android` 才取 Android 那几 GB。

`vkx fetch` 把 SDK 全部取回来（含 Android 那 1.1 GB）；`vkx doctor` 会逐项报告缺什么、怎么补。

### 只有两样东西装不进 ~/.vkx

| 缺什么 | 为什么 | 脚本会怎么做 |
| --- | --- | --- |
| macOS 的 Xcode / 命令行工具 | Apple 的 SDK 不允许第三方再分发，iOS 构建也必须用 `xcodebuild` | 提示执行 `xcode-select --install` |
| 显卡驱动里的 Vulkan ICD | Windows 的 `vulkan-1.dll`、Linux 的 ICD 都由驱动提供 | 提示更新驱动或装 `mesa-vulkan-drivers` |

其余的（C++ 编译器、CMake、Vulkan 相关的库和工具）都是可重定位的二进制，
一律装进 `~/.vkx`。Windows 上统一用 llvm-mingw，不需要 Visual Studio——
就算机器上装了也不用它：SDK 包里的预编译库是 llvm-mingw 编的，
两种 ABI 混在一起会在链接期炸，而 MSVC 的工具集也不允许我们分发。

## 建镜像

```sh
mirror/sync.sh mirror-root                       # 同步全部平台，约 5 GB
mirror/sync.sh mirror-root --platform macos-arm64  # 只同步一个平台
mirror/sync.sh mirror-root --skip android-ndk      # 跳过某些组件
VKX_LOCAL_BIN=target/release/vkx mirror/sync.sh mirror-root   # 还没发版时用本机编的 vkx

rsync -av --delete mirror-root/ root@yinli.tech:/var/www/file/   # 就是 vkx 默认的镜像
```

同一个上游文件服务多个平台时（macOS 的通用二进制、Windows 的 x64 包等）
只存一份，清单里多条记录指向同一个路径。

**注意 NDK 的宿主平台**：Google 只为 macOS、linux-x86_64、windows-x86_64
发布 NDK，所以 ARM64 的 Linux/Windows 机器无法构建 Android。

**同步最好直接在服务器上跑**：镜像要下几个 GB，服务器到上游的带宽通常比你本机
的上行快两个数量级（实测本机上传 ~1 MB/s，服务器同步 531 MB 只花了 40 秒）。
把 `sync.sh` 和安装脚本传上去，直接输出到 web 根目录即可：

```sh
rsync -a mirror/sync.sh install/install.sh install/install.ps1 root@host:/root/vkx-tools/
ssh root@host 'cd /root/vkx-tools && bash sync.sh /var/www/file'
```

还没发版时，可以把本机编好的 vkx 一起带上去，用 `VKX_LOCAL_PLATFORM` 标明它是哪个平台的：

```sh
VKX_LOCAL_BIN=/root/vkx-tools/vkx VKX_LOCAL_PLATFORM=macos-arm64 bash sync.sh /var/www/file
```

Caddy 那侧加一个分支就行：

```caddy
yinli.tech {
    handle_path /file/* {
        root * /var/www/file
        file_server
        header Cache-Control "public, max-age=86400"
    }
    # ...原有的其它 handle
}
```

产出的目录树：

```
manifest.txt                     组件 平台 版本 路径 sha256 安装目标
<组件>/<版本>/<组件>-<版本>-<平台>.tar.gz
```

每个包都被重新打包成统一格式——解开后就是安装目录里该有的内容，
安装脚本只需要「下载 → 校验 sha256 → 解压到目标位置」。升级依赖只改
`sync.sh` 顶部那段版本号，重跑同步即可。

镜像地址写在两个安装脚本开头的 `DEFAULT_MIRROR`，当前是 `https://yinli.tech/file`；
临时可以用 `VKX_MIRROR=<地址>` 覆盖（本地起个 `python3 -m http.server` 就能测）。

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

安装脚本只装 vkx 本身。其余全部来自 `vkx fetch` 取下来的 SDK 包——第一次
`vkx build` 会自动去取。

```
~/.vkx/
  bin/vkx
  cache/                         下载缓存
  sdk/toolchain/{cmake,ninja,slang,clang-format,llvm-mingw}
  sdk/vulkan/{vulkan,moltenvk}
  sdk/libs/{include,lib}         预编译的 C 库、头文件库、Vulkan-Headers、volk
  sdk/sources/{jolt,gamenetworking}
  sdk/android/{jdk,gradle,sdk}   移动端打包用（暂未进包）
```

`sdk/` 下一个目录对应清单里的一个组件，fetch 就是按这个对应关系解包的。
`VKX_HOME` 可以整体换个位置。

`sdk/vulkan/vulkan` 里是 Vulkan loader 和 khronos 校验层。校验层不在显卡驱动里，
Debug 构建要靠它报出用错 Vulkan 的地方，所以得自己分发。上游只有 LunarG 的整包
（每平台 274~493 MB，macOS / Windows 版还是安装脚本跑不动的 Qt 安装器），
于是由当时的 `vulkan-sdk.yml` 挑出需要的几个文件重打包成几十 MB，发到一个
单独的 Release，再由 `sync.sh` 的 `vulkan-sdk` 组件镜像过来。LunarG 不提供
Linux ARM64，那个平台在同一个 workflow 里从源码构建。

那个 workflow 和拼包用的 `sdk.yml` 现在都删了——包已经在镜像上，依赖也不会
再 rebase。要重建见 DEPLOY.md 顶部记的恢复方法。

### 运行期的环境变量

`vkx run` 启动程序前会补几个变量，指向 SDK 里的 Vulkan（见
`toolchain::vulkan_runtime_env`）。macOS 上设 `SDL_VULKAN_LIBRARY`（loader 的
绝对路径）和 `VK_DRIVER_FILES`：那是唯一连 loader 都没有的平台，不明确指定的话
SDL 会退而直接加载 MoltenVK，绕过 loader，校验层就无从插入——症状是程序照常
运行，只多一行「校验层不可用」。

**不能用 `DYLD_LIBRARY_PATH`。** macOS 的 SIP 会在执行受保护的系统二进制时把
`DYLD_*` 剥掉，中间隔一层 `/usr/bin/env` 或 `/bin/sh` 就没了。

Linux 和 Windows 的 loader 由显卡驱动提供，只补校验层的目录。

## 分发## 分发

`vkx dist` 出的是能直接发给别人的东西，全部落在工程的 `dist/` 下：

| 平台 | 产物 | 说明 |
| --- | --- | --- |
| macOS | `.app` + `.dmg` | MoltenVK 放进 `Contents/Frameworks/`——SDL 的默认搜索路径第一项就是这里，所以不需要任何额外代码或环境变量；ad-hoc 签名（上架或免右键打开还需公证） |
| Windows | `.zip` | 可执行文件静态链接，解压即用 |
| Linux | `.tar.gz` | 同上 |
| Android | 签名 `.apk` + `.aab` | APK 直接安装，AAB 上架 Google Play |
| iOS | `.ipa` | 需要 `vkx.toml` 里配好 `development_team` |

## 移动端开箱即用

**Android 签名**：`vkx new` 时就用 keytool 生成 `android/keystore/release.jks`
和随机口令，写进 `android/keystore.properties`（两者都在 .gitignore 里）。
所以 `vkx build --release --target android` 直接产出签名好的 APK。
上架商店请换成你自己保管的正式密钥。

**Xcode 工程**：iOS 构建本身就会生成 `.xcodeproj`（路径会打印出来），
用 Xcode 打开即可调试，和命令行构建是同一份配置。要连真机，在 `vkx.toml`
里填上团队 ID：

```toml
[ios]
development_team = "ABCDE12345"
```

填了之后真机构建会打开 Xcode 的自动签名。

## 开发

```sh
cargo build
cargo run -- new /tmp/demo
```

工程模版在 `template/`，用 `include_dir!` 在编译期整个嵌进二进制。
`build.rs` 声明了对该目录的依赖，改完模版直接 `cargo build` 就会重新嵌入。

发版：打一个 `v*` 标签，`.github/workflows/release.yml` 会为六个平台构建并
上传到 GitHub Release，再用 `sync.sh` 把它同步进镜像。

## 平台验证状态

| 平台 | 状态 |
| --- | --- |
| macOS (Apple Silicon) | 已验证：像素回读确认三角形；`.app` 在干净环境下用包内 MoltenVK 正常启动 |
| iOS 模拟器 | 已验证：构建 → 安装 → 启动 → 截图确认；`.ipa` 需要证书，未验证 |
| Android (arm64) | 已验证：签名 APK（apksigner 校验通过）+ AAB 产出；未在真机上跑过 |
| Windows / Linux | 未验证 |
