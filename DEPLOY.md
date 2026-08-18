# 部署到镜像

镜像是 `https://yinli.tech/file`（vkx 里的 `DEFAULT_MIRROR`），部署是手动的。
分两半，互相独立，可以分开更新：

```
/var/www/file/
  install.sh  install.ps1      读者 curl 的那两个脚本
  vkx/version.txt              最新版本号
  vkx/<版本>/vkx-<版本>-<平台> 六个平台的 vkx 二进制
  sdk/<平台>/manifest.txt       SDK 清单
  sdk/<平台>/sdk-<平台>.pack    拼接好的 SDK 包
```

`--delete` 只能打在 `vkx/` 或 `sdk/` 上，**不要打在根上**——根上这两棵树是
兄弟，打在根上会把另一棵连同安装脚本一起删掉。

## vkx 那一半

先 `git tag v<版本> && git push origin v<版本>` 让 release workflow 编出六个
平台的二进制，等它跑完，再：

```sh
sh mirror/publish-vkx.sh <版本> out
rsync -av --delete out/vkx/ root@yinli.tech:/var/www/file/vkx/
rsync -av out/install.sh out/install.ps1 root@yinli.tech:/var/www/file/
```

`publish-vkx.sh` 做的是格式翻译：release 发的是按 Rust 三元组命名的压缩包，
而 install.sh / install.ps1 / `vkx self update` 三个消费者要的是裸二进制加一个
`version.txt`。`version.txt` 最后才写——先写它的话，中途失败会让镜像指向一个
下不下来的版本。

## SDK 那一半

`gh workflow run sdk.yml --ref main` 跑完会发一个 `sdk-<编号>` release。
每个包在自己的 job 里逐字节比对过，release job 还会按发布后的地址再取一遍。

```sh
gh release download sdk-<编号> -R ozongzi/vkx -p site.tar
mkdir -p site && tar xf site.tar -C site
rsync -av --delete site/sdk/ root@yinli.tech:/var/www/file/sdk/
```

## 验一下

不改本机环境，指着镜像跑一遍读者的路径：

```sh
export VKX_MIRROR=https://yinli.tech/file HOME=$(mktemp -d)
curl -fsSL "$VKX_MIRROR/install.sh" | sh
"$HOME/.vkx/bin/vkx" new demo --package-id com.example.demo
cd "$HOME/demo" && "$HOME/.vkx/bin/vkx" run
```

窗口起来、日志里报出显卡型号、且**没有**「校验层不可用」，就算通了。
