# {{PROJECT_NAME}}

用 [vkx](https://github.com/ozongzi/vkx) 生成的 Vulkan + SDL3 工程。

## 跑起来

```sh
vkx run                        # 本机桌面（Windows / macOS / Linux）
vkx run --target android       # Android 真机或模拟器（需要 adb 能看到设备）
vkx run --target ios           # iOS 模拟器（顺带生成 Xcode 工程）
```

其它命令：

```sh
vkx build --release                    # Release 构建
vkx dist                               # 打成可以发给别人的安装包
vkx dist --target android              # 签名 APK + 上架用的 AAB
vkx clean                              # 删掉 build/
```

`vkx dist` 的产物都在 `dist/` 下：macOS 是 `.app` 和 `.dmg`（MoltenVK 已经
打进包里，对方机器上没装 Vulkan 也能跑），Windows 是 `.zip`，Linux 是
`.tar.gz`，Android 是签名 APK 和 AAB，iOS 是 `.ipa`。

## 签名与真机

**Android**：`vkx new` 已经生成好 `android/keystore/release.jks` 和随机口令
（记在 `android/keystore.properties`），release 构建自动签名。这两个文件都在
.gitignore 里，属于本机凭据；上架应用商店请换成你自己保管的正式密钥。

**iOS**：iOS 构建会在 `build/ios-simulator/`（或 `build/ios/`）下生成
`.xcodeproj`，可以直接用 Xcode 打开调试。要连真机，在 `vkx.toml` 里填上
Apple 开发者团队 ID，构建时就会打开自动签名：

```toml
[ios]
development_team = "ABCDE12345"
```

## 各文件是干什么的

| 路径 | 内容 |
| --- | --- |
| `src/main.cpp` | 全部代码：初始化 Vulkan、渲染循环、清理 |
| `shaders/triangle.slang` | 顶点和片元着色器 |
| `CMakeLists.txt` | 构建脚本，五个平台共用 |
| `cmake/VkxShaders.cmake` | 着色器编译规则：`.slang` → `.spv` → `.h` |
| `cmake/VkxEmbed.cmake` | 把 `.spv` 转成 C 数组的脚本 |
| `android/` | Gradle 工程；SDL3 的 `.aar` 由 vkx 自动放进 `app/libs/` |
| `ios/Info.plist` | iOS 应用包的配置 |
| `build/` | 构建产物，可随时删 |

## 运行时的几个机制

**Vulkan 函数从哪来。** 桌面和 Android 上由 volk 在运行期加载函数指针，入口点向
SDL 要；iOS 上 MoltenVK 是静态链接进二进制的，函数直接可用。分支在 `CMakeLists.txt`
和 `main.cpp` 顶部的 `VKX_STATIC_VULKAN` 宏。

**着色器怎么进到程序里。** 构建时 slangc 把 `.slang` 编成 SPIR-V，再转成 C 数组头文件，
被 `main.cpp` 直接 `#include`。运行时不读磁盘，也就不用管各平台的资源打包。

**画面怎么出来。** `drawFrame()` 每帧向交换链要一张图像，录制命令，提交到队列，再呈现。
两套「每帧资源」轮流用，让 CPU 录下一帧时 GPU 还在画上一帧。

**窗口大小变了怎么办。** 事件循环里置 `swapchainDirty_`，下一帧开头重建交换链。

## 接着往下加

- 改 `shaders/triangle.slang` 里的颜色，或 `main.cpp` 里的 `kVertices`
- 加顶点属性：`Vertex` 结构 + `createPipeline()` 的属性描述 + 着色器的 `VertexInput`
- 加纹理或 uniform：先在 `createPipeline()` 的管线布局里加描述符
- 加深度测试：`createSwapchain()` 里建深度图像，管线补 `pDepthStencilState`
- 新建的 Vulkan 对象，记得在 `shutdown()` 里销毁

`main.cpp` 里带「在这里……」的注释标出了这些位置。
