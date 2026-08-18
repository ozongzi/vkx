# vkx 要做到的七件事

安装脚本只装 vkx 本身。之后所有事情——取依赖、构建、打包、生成 IDE 工程、
自我更新——都由 vkx 负责。

---

## 1. 从指定站点取本平台的大包，且能只取一部分

每个平台一个 zip，里面是这个平台需要的全部东西：工具（cmake / ninja / slangc /
clang-format）、预编译的 C 库、头文件、要源码编的那两个库、Vulkan loader 和校验层、
MoltenVK、以及 Android 的 JDK / Gradle / SDK / NDK。

```sh
vkx fetch                          # 取本平台默认需要的部分
vkx fetch --component android      # 要出安卓包时才取那几 GB
vkx fetch --all
VKX_MIRROR=https://example.com/vkx vkx fetch    # 换站点
```

### 容器：多个 .tar.gz 首尾相接

原本想用 zip，但 zip 的中央目录在文件末尾——取中间一段拿到的字节不是合法
zip，还得自己实现解压。

gzip 流可以首尾相接（多成员 gzip，标准允许），于是改成：每个组件各压一个
`.tar.gz`，按顺序拼成一个文件。**每一段单独拿出来仍然是合法的 `.tar.gz`**，
`tar xzf` 直接能解，不用写任何解析代码。

### 打包时按组件排序，让每个组件是一段连续字节

如果条目在 zip 里是散的，取一个组件要发几千个 Range 请求。所以打包时把同一组件的
条目排在一起，于是：

```
sdk-macos-arm64.zip
  [0        .. 12 MB]   toolchain      cmake ninja slangc clang-format
  [12 MB    .. 31 MB]   libs           libSDL3.a libmbedtls.a … + include/
  [31 MB    .. 46 MB]   vulkan         loader + 校验层 + MoltenVK
  [46 MB    .. 88 MB]   sources        jolt/ GameNetworkingSockets/
  [88 MB    .. 4.9 GB]  android        JDK Gradle SDK NDK
```

**一个组件 = 一个 Range 请求。**

### 清单

和 zip 并排放一个小文件，vkx 先取它：

```json
{
  "platform": "macos-arm64",
  "zip": "sdk-macos-arm64.zip",
  "zip_sha256": "…",
  "components": {
    "toolchain": { "offset": 0,        "length": 12582912, "sha256": "…" },
    "libs":      { "offset": 12582912, "length": 19922944, "sha256": "…" },
    "android":   { "offset": 92274688, "length": 5033164800, "sha256": "…" }
  }
}
```

不去解析 zip 自己的中央目录，因为带 NDK 的包有二十万个条目，中央目录本身就有几十 MB。
自带清单只有几百字节。

取一个组件的流程：读清单 → 一次 `Range: bytes=offset-(offset+length-1)` →
校验 sha256 → 解开到 `~/.vkx/sdk/<组件>/`。中断可续传，校验不过就重来。

### 站点要求

只要支持 `Range`（Caddy 的 `file_server` 默认支持）。服务器忽略 `Range` 时会把
整个包发回来，vkx 按下载字节数当场拦下并说清原因，而不是等到 sha256 失败。

### 实测

用一个 22 MB 的测试包（toolchain 513 B / libs 2 MB / android 20 MB），
默认的 `vkx fetch` 只取桌面需要的两段：

```
/sdk/macos-arm64/manifest.txt        422
/sdk/macos-arm64/sdk-macos-arm64.pack        513
/sdk/macos-arm64/sdk-macos-arm64.pack    2001286
合计 2002221 字节 / 整包 22008595 字节 = 9.1%
```

再跑一次只传清单的 422 字节，其余全部跳过。

---

## 2. 自我更新

```sh
vkx self update              # 从默认站点取最新版
vkx self update --check      # 只看有没有新版
```

- Unix：写到临时文件、`chmod +x`、`rename` 覆盖。正在运行的进程持有旧 inode，安全。
- Windows：不能覆盖正在运行的 exe。先把自己 `rename` 成 `vkx.old.exe`，写入新的，
  下次启动时删掉旧的。

新版 vkx 对应新的 sdk 包时，更新完直接提示（并可以 `--fetch` 一起做掉）。
工程里不记任何依赖版本，所以升级是原子的，不存在部分升级。

---

## 3. 构建

```sh
vkx build [--release] [--target desktop|android|ios|ios-device]
vkx run   [--release] [-- 传给游戏的参数]
```

读 `vkx.toml`，生成 `target/CMakeLists.txt`，调 cmake + ninja。
详见 `build-config.md`。

---

## 4. 一键出各平台安装包

```sh
vkx dist                     # 本平台
vkx dist --target all
```

| 平台 | 产物 |
| --- | --- |
| macOS | `.app` + `.dmg`（MoltenVK 放进 `Contents/Frameworks/`，ad-hoc 签名） |
| Windows | `.zip`，静态链接，解压即用 |
| Linux | `.tar.gz`、`.deb`、`.rpm` |
| Android | 签名 `.apk` + `.aab` |
| iOS | `.ipa`（`vkx.toml` 里填了 `development_team` 才能出真机包） |

---

## 5. 生成 IDE 工程

```sh
vkx xcode          # 生成 target/ios/*.xcodeproj，并打开
vkx xcode --macos  # macOS 那份
```

根目录另外生成 `CMakePresets.json`，CLion / Visual Studio / VS Code 的 CMake
扩展打开工程目录就能直接构建调试。

---

## 6. help 要能当手册用

`vkx help` 不止列命令，还带专题页：

```sh
vkx help                 # 总览：命令 + 常见流程
vkx help build           # 单个命令的详细说明
vkx help toolchain       # 装了什么、在哪、占多少、怎么清
vkx help ios             # iOS 从零到真机的完整路径
vkx help android
vkx help mirror          # 怎么换站点、怎么自建
vkx help E0012           # 某个错误码的长篇解释
```

`vkx doctor` 检查环境并逐项报告：哪些组件在、版本对不对、Xcode 装没装、
显卡驱动有没有提供 Vulkan ICD。

---

## 7. 每个错误都要给出解法

现状：`Error::new` 有 44 处调用，约 13 处没有 hint；`From<io::Error>` 会把所有
IO 错误变成没有 hint 的裸消息。这两个口子要堵上。

### 让「没有解法的错误」编译不过

```rust
// 改成两个参数都必填，构造不出没有解法的错误
Error::new(what: impl Into<String>, fix: impl Into<String>)
```

### IO 错误必须带上下文

去掉 `From<std::io::Error>`，改成显式加上下文：

```rust
std::fs::read(&path).context("读取 {path}", "确认文件存在且有读权限")?
```

删掉自动转换之后，任何漏加上下文的地方直接编译不过。

### 错误码

```
error[E0012]: 找不到 Vulkan 运行时
  → macOS 需要 MoltenVK。运行 `vkx fetch --component vulkan` 取回来。
  → 详细说明：vkx help E0012
```

短消息保持一行，长篇放 `vkx help E0012`，也方便读者直接搜索码。

### 覆盖到的典型场景

| 情况 | 要给的解法 |
| --- | --- |
| 镜像连不上 | 换 `VKX_MIRROR`，或列出可用的备用站点 |
| 磁盘不够 | 说清还差多少，指出 `vkx clean --cache` 能腾出多少 |
| Xcode 没装 | `xcode-select --install`；真机还需要完整 Xcode 而非 CLT |
| 显卡没有 Vulkan ICD | Windows/Linux 更新驱动；Linux 装 `mesa-vulkan-drivers` |
| CMake 配置失败 | 把 CMake 的输出折叠起来，只把第一条 `CMake Error` 提到最前 |
| 编译错误 | 不复述编译器输出，只在错误来自生成的 CMake 时说明「这个文件是生成的，改 vkx.toml」 |
| 校验层报错 | 指出这是 Vulkan 用法错误，不是 vkx 的问题，并给出对应的规范条目链接 |
