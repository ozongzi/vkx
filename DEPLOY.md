# 部署到镜像

> **打包 CI 已经删掉。** `sdk.yml`（六平台拼包）和 `vulkan-sdk.yml`（重打
> LunarG 的包）都是一次性的手段——包已经产出、已经在镜像上了，依赖也不会再
> rebase，留着只是每跑一轮多发一个用不上的 release。
>
> 现在 CI 只剩 `release.yml`，做一件事：交叉编译 vkx 自己，外加一道脚本体检。
>
> 真要重建包的话，两个 workflow 在 git 历史里：
>
> ```sh
> git show 93d2b13:.github/workflows/sdk.yml        > sdk.yml
> git show 82f6847:.github/workflows/vulkan-sdk.yml > vulkan-sdk.yml
> ```
>
> 挑文件、编库、拼包、自检的逻辑本来就不在 workflow 里，而在 `mirror/*.sh`，
> 那些都还在。workflow 里只有 runner 矩阵和交叉编译的开关。


镜像是 `https://yinli.tech/file`（vkx 里的 `DEFAULT_MIRROR`）。

**在服务器上做，不要在本机做。** 服务器到 GitHub 实测 26 MB/s，本机上行约
1 MB/s——差两个数量级。要传的是几个 GB，路径必须是「上游/GitHub → 服务器」，
不能绕经谁的笔记本。

服务器：Ubuntu 24.04，**1 核 1 GB**。这个配置决定了下面几处取舍。

## 镜像长什么样

```
/var/www/file/
  install.sh  install.ps1        读者 curl 的那两个脚本
  vkx/version.txt                最新版本号
  vkx/<版本>/vkx-<版本>-<平台>   六个平台的 vkx 二进制
  sdk/<平台>/manifest.txt        SDK 清单
  sdk/<平台>/sdk-<平台>.pack     拼接好的 SDK 包

  cmake/ jdk/ android-ndk/ …     sync.sh 铺的旧式按组件目录树（约 5 GB）
```

最后那棵旧树读者已经用不到了，但**别删**：安卓组件就是从它里面组装的。

`--delete` 只能打在 `vkx/` 或 `sdk/` 上，**不要打在根上**——这几棵是兄弟，
打在根上会把别的连同安装脚本一起删掉。

## vkx 那一半

先 `git tag v<版本> && git push origin v<版本>`，等 release workflow 编完六个
平台的二进制，然后在服务器上：

```sh
sh mirror/publish-vkx.sh <版本> /root/work/out
rsync -av --delete /root/work/out/vkx/ /var/www/file/vkx/
cp /root/work/out/install.sh /root/work/out/install.ps1 /var/www/file/
```

`publish-vkx.sh` 做的是格式翻译：release 发的是按 Rust 三元组命名的压缩包，
而 install.sh / install.ps1 / `vkx self update` 三个消费者要的是裸二进制加一个
`version.txt`。`version.txt` 最后才写——先写它的话，中途失败会让镜像指向一个
下不下来的版本。

## SDK 那一半

`gh workflow run sdk.yml --ref main` 跑完会发一个 `sdk-<编号>` release，
六个平台的包各自逐字节自检过。在服务器上取下来：

```sh
mkdir -p /root/work/site/sdk && cd /root/work/site/sdk
for p in macos-arm64 macos-x64 linux-x64 linux-arm64 windows-x64 windows-arm64; do
    mkdir -p "$p"
    curl -sSL -o "$p/sdk-$p.pack"  ".../releases/download/sdk-<编号>/sdk-$p.pack"
    curl -sSL -o "$p/manifest.txt" ".../releases/download/sdk-<编号>/manifest-$p.txt"
done
```

### 安卓组件在服务器上追加，不走 CI

JDK、Gradle、cmdline-tools、platform-tools、build-tools、platform、NDK
**全是上游现成的二进制，没有一行需要编译**。那条六平台的 CI 矩阵存在的理由是
交叉编译 `libs`，安卓走它毫无意义，还要多绕 GitHub 一趟。而这些东西旧镜像树里
早就有了。

包是各段首尾相接，`pack.sh` 的 `ORDER` 把 android 排在最后，所以追加一段就是
`cat` 一次加一行清单——不用重打整个包，桌面读者用 Range 也永远取不到它。

```sh
for p in macos-arm64 macos-x64 linux-x64 windows-x64; do
    sh mirror/build-android.sh "$p" /root/work/stage
    ZSTD_LEVEL=10 sh mirror/append-component.sh \
        "/root/work/site/sdk/$p/sdk-$p.pack" \
        "/root/work/site/sdk/$p/manifest.txt" \
        android /root/work/stage
    rm -rf /root/work/stage
done
rsync -av --delete /root/work/site/sdk/ /var/www/file/sdk/
```

只有四个平台。Google 只为 macOS、linux-x86_64、windows-x86_64 发 NDK，
`linux-arm64` 和 `windows-arm64` 本来就构建不了 Android，`build-android.sh`
会自己跳过——把剩下那 580 MB 塞进去只是让人白下。

`ZSTD_LEVEL=10` 是量出来的。镜像机单核，300 MB 的 NDK 数据：

| 级别 | 大小 | 耗时 |
|---|---|---|
| -19 | 57.6 MB | 386 秒 |
| -10 | 65.4 MB | 25 秒 |
| -6 | 69.0 MB | 15 秒 |

`-19` 只小 12%，慢 15 倍；整个组件换算下来是每平台省 81 MB、多花一个多小时。
里面大半是已经压过的 NDK 二进制，再挤没多少油水。级别对 vkx 不可见——它按魔数
认格式。

因此**镜像上的包和 GitHub 上的 `sdk-<编号>` 不是同一份**：GitHub 那份是 CI 编
出来的底座，镜像这份在底座上多接了一段 android。

## 验一下

不改本机环境，指着真实镜像跑一遍读者的路径：

```sh
export VKX_MIRROR=https://yinli.tech/file HOME=$(mktemp -d)
curl -fsSL "$VKX_MIRROR/install.sh" | sh
"$HOME/.vkx/bin/vkx" new demo --package-id com.example.demo
cd "$HOME/demo" && "$HOME/.vkx/bin/vkx" run
```

窗口起来、日志里报出显卡型号、且**没有**「校验层不可用」，就算通了。
追加过 android 的平台再补一句 `vkx fetch --component android`——它会校验
那一段的 sha256，偏移算错的话这里就会炸。
