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
| **安装脚本** | 从镜像把整套开发环境装到 `~/.vkx`：vkx、CMake、Ninja、slangc、JDK、Gradle、Android SDK/NDK、MoltenVK、llvm-mingw、以及 SDL3 等依赖的源码 |
| **vkx** | 只使用环境，自己不下载任何东西。缺件时报错指向重跑安装脚本 |
| **镜像** | 你自己的服务器。`mirror/sync.sh` 把上游同步成一棵可直接对外提供的目录树 |

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

加 `--no-android`（PowerShell 是 `-NoAndroid`）可以跳过 Android 部分，省约 5 GB。
脚本最后会自检一遍（每个装好的工具都实际跑一次），有问题当场报出来。

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

rsync -av --delete mirror-root/ user@host:/var/www/file/   # 对应 yinli.tech/file
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

```
~/.vkx/
  bin/vkx
  env.sh                       PATH / JAVA_HOME / ANDROID_HOME，安装脚本接进你的 shell
  installed.txt                已装组件和版本，用于增量升级
  tools/{cmake,ninja,slang,clang-format,jdk,gradle,moltenvk,vulkan,llvm-mingw}
  android/sdk/{cmdline-tools,platform-tools,build-tools,platforms,ndk}
  src/{sdl3,sdl3-android,vulkan-headers,volk}    构建时离线取用
```

`tools/vulkan` 里是 Vulkan loader 和 khronos 校验层。校验层不在显卡驱动里，
Debug 构建要靠它报出用错 Vulkan 的地方，所以得自己分发。上游只有 LunarG 的整包
（每平台 274~493 MB，macOS / Windows 版还是安装脚本跑不动的 Qt 安装器），
于是由 `.github/workflows/vulkan-sdk.yml` 挑出需要的几个文件重打包成几十 MB，
发到一个单独的 Release，再由 `sync.sh` 的 `vulkan-sdk` 组件镜像过来。
LunarG 不提供 Linux ARM64，那个平台在同一个 workflow 里从源码构建。

Vulkan 版本升级时手动触发那个 workflow，改 `sync.sh` 顶部的 `VULKAN_SDK` 再重跑同步。

macOS 上还额外设了 `SDL_VULKAN_LIBRARY`（loader 的绝对路径）和 `VK_DRIVER_FILES`：
那是唯一连 loader 都没有的平台，不明确指定的话 SDL 会退而直接加载 MoltenVK，
绕过 loader，校验层就无从插入。这几个变量都不用 `DYLD_*`——macOS 的 SIP 会在执行
系统二进制时把 `DYLD_*` 剥掉，只要中间隔一层 `/bin/sh` 就失效。

`src/` 里的源码通过 CMake 的 `FETCHCONTENT_SOURCE_DIR_*` 直接喂给工程，
所以构建全程不联网，产物也不会依赖机器上装的系统 SDL3。

vkx 自己不注入任何环境变量，运行时原样继承你 shell 里的环境——所以上面那几个
变量都由 `env.sh` 提供，安装脚本会把它接进你的 shell。

## 分发

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
