//! SDK 依赖表——编进二进制的那份。
//!
//! # 为什么表在二进制里，不在网上
//!
//! 依赖不追版本：一个 vkx 版本对应一套确定的依赖，钉死了就不动，出 CVE 也不动。
//! 既然如此，「这一版要哪些东西、每样的哈希是多少」就是 vkx 自己的一部分，
//! 没有理由再去网上取一份清单——那只会多一个能失败、能被篡改、能和二进制对不上
//! 的环节。要换依赖就发新版 vkx。
//!
//! # 为什么直接下上游的预编译包
//!
//! 以前是我们把上游包重新打一遍、传到自己的镜像上。这样安装端最省事（解开就是
//! 该有的样子），代价是每个字节都走我们的服务器。现在反过来：认上游的包，剥壳
//! 的逻辑放进 vkx，字节走清华、腾讯这些机构镜像。
//!
//! 只有四样没有国内镜像，必须自己托管：slang、vulkan-sdk、moltenvk，以及我们
//! 自己编的 libs。
//!
//! # 为什么是 blake3
//!
//! 比 sha256 快一个数量级，而这些包解压前要整个过一遍哈希——NDK 那种上 GB 的
//! 东西，差别是能感觉到的。上游不发 blake3，表里这些值由 `vkx dev hash` 跑一遍
//! 全部 URL 算出来。

/// vkx 自己能跑在哪些机器上。
///
/// 只有三个。arm64 的 Windows/Linux 和 x64 的 macOS 都不在列——前两者没人拿来
/// 开发游戏，后者 Apple 自己已经停售四年。少一个平台就少一套要验的组合。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    WindowsX64,
    LinuxX64,
    MacosArm64,
}

impl Host {
    pub const ALL: &'static [Host] = &[Host::WindowsX64, Host::LinuxX64, Host::MacosArm64];

    pub fn name(self) -> &'static str {
        match self {
            Host::WindowsX64 => "windows-x64",
            Host::LinuxX64 => "linux-x64",
            Host::MacosArm64 => "macos-arm64",
        }
    }

    /// 本机是哪个。不认识就是 None——vkx 在这台机器上跑不了，早说比晚说好。
    pub fn detect() -> Option<Host> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Some(Host::WindowsX64),
            ("linux", "x86_64") => Some(Host::LinuxX64),
            ("macos", "aarch64") => Some(Host::MacosArm64),
            _ => None,
        }
    }
}

/// 游戏能出到哪些平台。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    WindowsX64,
    LinuxX64,
    MacosArm64,
    AndroidArm64,
    AndroidX64,
    IosArm64,
}

impl Target {
    pub const ALL: &'static [Target] = &[
        Target::WindowsX64,
        Target::LinuxX64,
        Target::MacosArm64,
        Target::AndroidArm64,
        Target::AndroidX64,
        Target::IosArm64,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Target::WindowsX64 => "windows-x64",
            Target::LinuxX64 => "linux-x64",
            Target::MacosArm64 => "macos-arm64",
            Target::AndroidArm64 => "android-arm64",
            Target::AndroidX64 => "android-x64",
            Target::IosArm64 => "ios-arm64",
        }
    }

    /// 这个目标能不能在这台机器上构建。
    ///
    /// 桌面三家各管各的（交叉编译 C++ 加系统库不值得）；安卓靠 NDK，三个开发
    /// 平台都行；iOS 要 Xcode 和苹果的签名链，只有 macOS。
    pub fn buildable_on(self, host: Host) -> bool {
        match self {
            Target::WindowsX64 => host == Host::WindowsX64,
            Target::LinuxX64 => host == Host::LinuxX64,
            Target::MacosArm64 => host == Host::MacosArm64,
            Target::AndroidArm64 | Target::AndroidX64 => true,
            Target::IosArm64 => host == Host::MacosArm64,
        }
    }
}

/// 下载回来的包长什么样，决定怎么解。
#[derive(Clone, Copy)]
pub enum Format {
    TarGz,
    TarXz,
    TarZst,
    Tar,
    /// zip，也包括 Python 的 .whl——它就是个 zip。
    Zip,
}

// ===========================================================================
// 依赖表
// ===========================================================================
// 版本钉死在这里。安装包里带的就是这些，一个 vkx 版本对应一套确定的依赖，
// 钉死了就不动，出 CVE 也不动——想换就发新版 vkx，没有别的路径能改动读者
// 机器上装的是什么。

/// 一个组件在某个开发平台上的全部信息。
///
/// `blake3` 是安装包里那个文件的哈希，不是装完之后那棵目录树的。树哈希
/// 每次跑都要过一遍几 GB，不现实；装的时候校验来源、装完记一个戳，够用。
pub struct Entry {
    pub name: &'static str,
    pub host: Host,
    /// 安装包 deps/ 下的文件名。
    pub file: &'static str,
    pub format: Format,
    /// 装到 ~/.vkx/sdk/ 下的哪里。
    pub dest: &'static str,
    /// 解开后从哪一层往下才是要的。空表示自动剥掉单层外壳。
    pub pick: &'static str,
    pub blake3: &'static str,
    pub about: &'static str,
    /// 这个包当初从哪儿来的。运行时用不到，是给重新打安装包时看的。
    pub origin: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        name: "sources", host: Host::MacosArm64,
        file: "sources.tar.zst",
        format: Format::TarZst, dest: "sources", pick: "",
        blake3: "fa2161778a7f7882448f07caa7821670e1e66da02d2f12185348614fb28a1470",
        about: "Jolt / GameNetworkingSockets 源码（vkx add 打开才用）",
        origin: "https://github.com/jrouwe/JoltPhysics v5.2.0 + https://github.com/ValveSoftware/GameNetworkingSockets v1.4.1，砍掉 Docs/Samples/UnitTests/.github",
    },
    Entry {
        name: "sources", host: Host::LinuxX64,
        file: "sources.tar.zst",
        format: Format::TarZst, dest: "sources", pick: "",
        blake3: "fa2161778a7f7882448f07caa7821670e1e66da02d2f12185348614fb28a1470",
        about: "Jolt / GameNetworkingSockets 源码（vkx add 打开才用）",
        origin: "https://github.com/jrouwe/JoltPhysics v5.2.0 + https://github.com/ValveSoftware/GameNetworkingSockets v1.4.1，砍掉 Docs/Samples/UnitTests/.github",
    },
    Entry {
        name: "sources", host: Host::WindowsX64,
        file: "sources.tar.zst",
        format: Format::TarZst, dest: "sources", pick: "",
        blake3: "fa2161778a7f7882448f07caa7821670e1e66da02d2f12185348614fb28a1470",
        about: "Jolt / GameNetworkingSockets 源码（vkx add 打开才用）",
        origin: "https://github.com/jrouwe/JoltPhysics v5.2.0 + https://github.com/ValveSoftware/GameNetworkingSockets v1.4.1，砍掉 Docs/Samples/UnitTests/.github",
    },
    Entry {
        name: "jdk", host: Host::MacosArm64,
        file: "OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.12_8.tar.gz",
        format: Format::TarGz, dest: "android/jdk", pick: "Contents/Home",
        blake3: "9a0aab2ebfb1c81ccf270216ae327baed84813e9c4810f1f214eb6ad83db3004",
        about: "Gradle 和 javac 的运行时",
        origin: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/aarch64/mac/OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.12_8.tar.gz",
    },
    Entry {
        name: "android-ndk", host: Host::MacosArm64,
        file: "android-ndk-r28c-darwin.zip",
        format: Format::Zip, dest: "android/sdk/ndk/28.2.13676358", pick: "",
        blake3: "1655ed88c27b4a74245495cb8cfef73cbe97e004ac67ede1dc329b4e6587ed23",
        about: "安卓 C++ 交叉编译",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/android-ndk-r28c-darwin.zip",
    },
    Entry {
        name: "android-build-tools", host: Host::MacosArm64,
        file: "build-tools_r36.1_macosx.zip",
        format: Format::Zip, dest: "android/sdk/build-tools/36.1.0", pick: "",
        blake3: "5e794b79530d5e22501a2f8134d00b14810d09152a5148dc50a13dd5a5223baa",
        about: "aapt2 / d8 / apksigner",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/build-tools_r36.1_macosx.zip",
    },
    Entry {
        name: "clang-format", host: Host::MacosArm64,
        file: "clang_format-22.1.8-py2.py3-none-macosx_11_0_arm64.whl",
        format: Format::Zip, dest: "toolchain/clang-format", pick: "clang_format/data/bin",
        blake3: "566ba3f1c16571c5513eb4ee4db9a09fdee20c763ea180d0e7d9a2c0f0a6f8e4",
        about: "代码格式化",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/2e/55/539cc1036dae16659f50500ca34838cc5b16cd3e98e3faaf164186b98093/clang_format-22.1.8-py2.py3-none-macosx_11_0_arm64.whl",
    },
    Entry {
        name: "cmake", host: Host::MacosArm64,
        file: "cmake-4.1.2-py3-none-macosx_10_10_universal2.whl",
        format: Format::Zip, dest: "toolchain/cmake", pick: "cmake/data",
        blake3: "4f4ee227e1b7732f6e812e99228189bc099b52803c381c5195b784b4aa1b7f92",
        about: "构建系统",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/ca/f7/f28a1df8d35cb6e37ff087e9f995cc0253ab1ffc55b12cf276436db4d392/cmake-4.1.2-py3-none-macosx_10_10_universal2.whl",
    },
    Entry {
        name: "android-cmdline-tools", host: Host::MacosArm64,
        file: "commandlinetools-mac-11076708_latest.zip",
        format: Format::Zip, dest: "android/sdk/cmdline-tools/latest", pick: "",
        blake3: "936fd71ebd550a604d574b47df483abc2e7bca388b5ac95640773e9b41104f6f",
        about: "sdkmanager 等",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/commandlinetools-mac-11076708_latest.zip",
    },
    Entry {
        name: "gradle", host: Host::MacosArm64,
        file: "gradle-8.13-bin.zip",
        format: Format::Zip, dest: "android/gradle", pick: "",
        blake3: "9de2537ea1a951d98117f22f18a24e3614023e7687d82e7429081e5a670c9243",
        about: "安卓构建",
        origin: "https://mirrors.cloud.tencent.com/gradle/gradle-8.13-bin.zip",
    },
    Entry {
        name: "libs", host: Host::MacosArm64,
        file: "libs.tar.zst",
        format: Format::TarZst, dest: "libs", pick: "",
        blake3: "375060b31551da7e3da477500a2551a8cd85fcb90713ef8593fb0d9c8592449e",
        about: "预编译 C 库和头文件",
        origin: "",
    },
    Entry {
        name: "moltenvk", host: Host::MacosArm64,
        file: "moltenvk.tar.gz",
        format: Format::TarGz, dest: "vulkan/moltenvk", pick: "",
        blake3: "b657724bb557128c54a609fe301810ace8c7589dd75238e384a04f597e5f88e3",
        about: "Vulkan 翻译成 Metal",
        origin: "",
    },
    Entry {
        name: "ninja", host: Host::MacosArm64,
        file: "ninja-1.13.0-py3-none-macosx_10_9_universal2.whl",
        format: Format::Zip, dest: "toolchain/ninja", pick: "ninja-1.13.0.data/scripts",
        blake3: "a9cb6f1feddb6e901cf92dda0a53b81f36712205951376b08cfb035c5d4e1ed2",
        about: "构建驱动",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/3c/74/d02409ed2aa865e051b7edda22ad416a39d81a84980f544f8de717cab133/ninja-1.13.0-py3-none-macosx_10_9_universal2.whl",
    },
    Entry {
        name: "android-platform", host: Host::MacosArm64,
        file: "platform-36_r01.zip",
        format: Format::Zip, dest: "android/sdk/platforms/android-36", pick: "",
        blake3: "821aecd450e4bdb1cb810e210e89bdaa70ea7f42d3a41d1baf0dfb85f7485be9",
        about: "android.jar (API 36)",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-36_r01.zip",
    },
    Entry {
        name: "android-platform-tools", host: Host::MacosArm64,
        file: "platform-tools-latest-darwin.zip",
        format: Format::Zip, dest: "android/sdk/platform-tools", pick: "",
        blake3: "68eb5d346be302b56377d69e0b42adbbf81a6bcedbd3cb6df390c00788b7eb9e",
        about: "adb",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-tools-latest-darwin.zip",
    },
    Entry {
        name: "slang", host: Host::MacosArm64,
        file: "slang.tar.gz",
        format: Format::TarGz, dest: "toolchain/slang", pick: "",
        blake3: "e409f2f9535f0900e91eed00ae268b066fdbd6722eaea3349802f958ef5256d3",
        about: "着色器编译器",
        origin: "",
    },
    Entry {
        name: "vulkan-sdk", host: Host::MacosArm64,
        file: "vulkan-sdk.tar.gz",
        format: Format::TarGz, dest: "vulkan/vulkan", pick: "",
        blake3: "a8cc4a37dec24d8190d5795c61888963bcac4548186c990e66c4abcd29b02854",
        about: "loader + 校验层",
        origin: "",
    },
    Entry {
        name: "jdk", host: Host::LinuxX64,
        file: "OpenJDK21U-jdk_x64_linux_hotspot_21.0.12_8.tar.gz",
        format: Format::TarGz, dest: "android/jdk", pick: "",
        blake3: "f20bd76d057d9aa1e656f9b0905ae55a4eaf847656bb08dc71320f720f2faedf",
        about: "Gradle 和 javac 的运行时",
        origin: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/linux/OpenJDK21U-jdk_x64_linux_hotspot_21.0.12_8.tar.gz",
    },
    Entry {
        name: "android-ndk", host: Host::LinuxX64,
        file: "android-ndk-r28c-linux.zip",
        format: Format::Zip, dest: "android/sdk/ndk/28.2.13676358", pick: "",
        blake3: "b7dad94488089543d4d63c162e4fb266ffe65c6332c2fef191047417bfb2d136",
        about: "安卓 C++ 交叉编译",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/android-ndk-r28c-linux.zip",
    },
    Entry {
        name: "android-build-tools", host: Host::LinuxX64,
        file: "build-tools_r36.1_linux.zip",
        format: Format::Zip, dest: "android/sdk/build-tools/36.1.0", pick: "",
        blake3: "1f7b2f757774e60daeb2fe51ee9655a6250ee29d695e76d0ff439e173864618e",
        about: "aapt2 / d8 / apksigner",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/build-tools_r36.1_linux.zip",
    },
    Entry {
        name: "clang-format", host: Host::LinuxX64,
        file: "clang_format-22.1.8-py2.py3-none-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
        format: Format::Zip, dest: "toolchain/clang-format", pick: "clang_format/data/bin",
        blake3: "c2b50d9009e1ca0c28099f3588830be9e99b75e922b4c0cf1a6338a21e7053c8",
        about: "代码格式化",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/e5/88/b82c066fa807da4ca2518fecf79071361f6324b77375e5e92c059c0697fd/clang_format-22.1.8-py2.py3-none-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
    },
    Entry {
        name: "cmake", host: Host::LinuxX64,
        file: "cmake-4.1.2-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
        format: Format::Zip, dest: "toolchain/cmake", pick: "cmake/data",
        blake3: "22b8497deadf129f413683ec1a86508f71a2b467c6c4486bcb42bd9edf0a4d23",
        about: "构建系统",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/f3/56/0fc4d83f212cef10b7bbf6c5043e4582af80ad2aef6905e0dc33fbf68b11/cmake-4.1.2-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
    },
    Entry {
        name: "android-cmdline-tools", host: Host::LinuxX64,
        file: "commandlinetools-linux-11076708_latest.zip",
        format: Format::Zip, dest: "android/sdk/cmdline-tools/latest", pick: "",
        blake3: "8f2b46a25ffdcd5e1724f28e2a6b11bef4e2da747499b30675e8c487e14c2915",
        about: "sdkmanager 等",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/commandlinetools-linux-11076708_latest.zip",
    },
    Entry {
        name: "gradle", host: Host::LinuxX64,
        file: "gradle-8.13-bin.zip",
        format: Format::Zip, dest: "android/gradle", pick: "",
        blake3: "9de2537ea1a951d98117f22f18a24e3614023e7687d82e7429081e5a670c9243",
        about: "安卓构建",
        origin: "https://mirrors.cloud.tencent.com/gradle/gradle-8.13-bin.zip",
    },
    Entry {
        name: "libs", host: Host::LinuxX64,
        file: "libs.tar.zst",
        format: Format::TarZst, dest: "libs", pick: "",
        blake3: "ef37e484f5c8ec1504fb2c074a8c80913866139f7c18b891b8d0d60f331a5ab4",
        about: "预编译 C 库和头文件",
        origin: "",
    },
    Entry {
        name: "ninja", host: Host::LinuxX64,
        file: "ninja-1.13.0-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
        format: Format::Zip, dest: "toolchain/ninja", pick: "ninja-1.13.0.data/scripts",
        blake3: "44aa9a9fa3dc9b3e0a69500caf4936b8e729f02caed32635c0309526851e3892",
        about: "构建驱动",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/ed/de/0e6edf44d6a04dabd0318a519125ed0415ce437ad5a1ec9b9be03d9048cf/ninja-1.13.0-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
    },
    Entry {
        name: "android-platform", host: Host::LinuxX64,
        file: "platform-36_r01.zip",
        format: Format::Zip, dest: "android/sdk/platforms/android-36", pick: "",
        blake3: "821aecd450e4bdb1cb810e210e89bdaa70ea7f42d3a41d1baf0dfb85f7485be9",
        about: "android.jar (API 36)",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-36_r01.zip",
    },
    Entry {
        name: "android-platform-tools", host: Host::LinuxX64,
        file: "platform-tools-latest-linux.zip",
        format: Format::Zip, dest: "android/sdk/platform-tools", pick: "",
        blake3: "05cad8db65eba2d168baeed053ff7fc1304dbe6cb6c10ddfbdd16e737c82b6ed",
        about: "adb",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-tools-latest-linux.zip",
    },
    Entry {
        name: "slang", host: Host::LinuxX64,
        file: "slang.tar.gz",
        format: Format::TarGz, dest: "toolchain/slang", pick: "",
        blake3: "d237699fbb0ec1b90ce8bbc8971654e71a35fe9ff4da1a2b2d874e954ae22321",
        about: "着色器编译器",
        origin: "",
    },
    Entry {
        name: "vulkan-sdk", host: Host::LinuxX64,
        file: "vulkan-sdk.tar.gz",
        format: Format::TarGz, dest: "vulkan/vulkan", pick: "",
        blake3: "151ae62b52a67b78a978490b1bb13f61f3fb927b606509140ad24c33d71ae359",
        about: "loader + 校验层",
        origin: "",
    },
    Entry {
        name: "jdk", host: Host::WindowsX64,
        file: "OpenJDK21U-jdk_x64_windows_hotspot_21.0.12_8.zip",
        format: Format::Zip, dest: "android/jdk", pick: "",
        blake3: "fccbe84d69cf338c4da68a1b6db59b6562f78cf0f308bca64f79ec86192d3828",
        about: "Gradle 和 javac 的运行时",
        origin: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/windows/OpenJDK21U-jdk_x64_windows_hotspot_21.0.12_8.zip",
    },
    Entry {
        name: "android-ndk", host: Host::WindowsX64,
        file: "android-ndk-r28c-windows.zip",
        format: Format::Zip, dest: "android/sdk/ndk/28.2.13676358", pick: "",
        blake3: "d65222f333544f0cd33cc1ef8aafc9d9ae06ade95d61af6bbd35a7649a2663d4",
        about: "安卓 C++ 交叉编译",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/android-ndk-r28c-windows.zip",
    },
    Entry {
        name: "android-build-tools", host: Host::WindowsX64,
        file: "build-tools_r36.1_windows.zip",
        format: Format::Zip, dest: "android/sdk/build-tools/36.1.0", pick: "",
        blake3: "004bb40f2dc1b3aa100ca2e84e1770d4db9a1c3566101247dca5741c828b1a25",
        about: "aapt2 / d8 / apksigner",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/build-tools_r36.1_windows.zip",
    },
    Entry {
        name: "clang-format", host: Host::WindowsX64,
        file: "clang_format-22.1.8-py2.py3-none-win_amd64.whl",
        format: Format::Zip, dest: "toolchain/clang-format", pick: "clang_format/data/bin",
        blake3: "107f171dc063b6568275a0738967cb2a2ef39a5b1ff4b406ed6d0b3a9057d6ab",
        about: "代码格式化",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/08/60/c6783b3190a8f741107a44912a11c39c1a51e254e86a4c43cb0151cea0dd/clang_format-22.1.8-py2.py3-none-win_amd64.whl",
    },
    Entry {
        name: "cmake", host: Host::WindowsX64,
        file: "cmake-4.1.2-py3-none-win_amd64.whl",
        format: Format::Zip, dest: "toolchain/cmake", pick: "cmake/data",
        blake3: "5a4898a9ed4a405edff5de2b3aef92176424d1cb325225312ef9b48030043dc7",
        about: "构建系统",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/be/36/77db223c6619aa11817040d9b0a9c232c9e26c86780cd676ff83e110ae73/cmake-4.1.2-py3-none-win_amd64.whl",
    },
    Entry {
        name: "android-cmdline-tools", host: Host::WindowsX64,
        file: "commandlinetools-win-11076708_latest.zip",
        format: Format::Zip, dest: "android/sdk/cmdline-tools/latest", pick: "",
        blake3: "64b8a85ca65b489eeb5454c641a086436028a505c9a60fed2b12496f289d5ed2",
        about: "sdkmanager 等",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/commandlinetools-win-11076708_latest.zip",
    },
    Entry {
        name: "gradle", host: Host::WindowsX64,
        file: "gradle-8.13-bin.zip",
        format: Format::Zip, dest: "android/gradle", pick: "",
        blake3: "9de2537ea1a951d98117f22f18a24e3614023e7687d82e7429081e5a670c9243",
        about: "安卓构建",
        origin: "https://mirrors.cloud.tencent.com/gradle/gradle-8.13-bin.zip",
    },
    Entry {
        name: "libs", host: Host::WindowsX64,
        file: "libs.tar.zst",
        format: Format::TarZst, dest: "libs", pick: "",
        blake3: "4d54c7858b53a02f7e435a5f868aced530a87f5bd045d8d9bf2bc3d650ddcc1c",
        about: "预编译 C 库和头文件",
        origin: "",
    },
    Entry {
        name: "llvm-mingw", host: Host::WindowsX64,
        file: "llvm-mingw-20250910-ucrt-x86_64.zip",
        format: Format::Zip, dest: "toolchain/llvm-mingw", pick: "",
        blake3: "e1f027cbf0260b9ad822e8c41faeb41d80aa36751dcb71ada7dd42748c2d215e",
        about: "Windows 上的 C++ 编译器",
        origin: "https://github.com/mstorsjo/llvm-mingw/releases/download/20250910/llvm-mingw-20250910-ucrt-x86_64.zip",
    },
    Entry {
        name: "ninja", host: Host::WindowsX64,
        file: "ninja-1.13.0-py3-none-win_amd64.whl",
        format: Format::Zip, dest: "toolchain/ninja", pick: "ninja-1.13.0.data/scripts",
        blake3: "e7aa4507778b58c3c731073b7b1056759a15819e1f1b2acd1db2db1db6ac24df",
        about: "构建驱动",
        origin: "https://pypi.tuna.tsinghua.edu.cn/packages/29/45/c0adfbfb0b5895aa18cec400c535b4f7ff3e52536e0403602fc1a23f7de9/ninja-1.13.0-py3-none-win_amd64.whl",
    },
    Entry {
        name: "android-platform", host: Host::WindowsX64,
        file: "platform-36_r01.zip",
        format: Format::Zip, dest: "android/sdk/platforms/android-36", pick: "",
        blake3: "821aecd450e4bdb1cb810e210e89bdaa70ea7f42d3a41d1baf0dfb85f7485be9",
        about: "android.jar (API 36)",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-36_r01.zip",
    },
    Entry {
        name: "android-platform-tools", host: Host::WindowsX64,
        file: "platform-tools-latest-windows.zip",
        format: Format::Zip, dest: "android/sdk/platform-tools", pick: "",
        blake3: "b86bdd1b2bf4c3f8c0e9c4e298ee2a8cd3496d8f5d5f3d53ab566fdefd1435ec",
        about: "adb",
        origin: "https://mirrors.cloud.tencent.com/AndroidSDK/platform-tools-latest-windows.zip",
    },
    Entry {
        name: "slang", host: Host::WindowsX64,
        file: "slang.tar.gz",
        format: Format::TarGz, dest: "toolchain/slang", pick: "",
        blake3: "2f9c82a9d91655c20cd1b141de30782b8a5ea3dc895d2bf80e1007601ce1f88d",
        about: "着色器编译器",
        origin: "",
    },
    Entry {
        name: "vulkan-sdk", host: Host::WindowsX64,
        file: "vulkan-sdk.tar.gz",
        format: Format::TarGz, dest: "vulkan/vulkan", pick: "",
        blake3: "e273b83a4533d4780e32e773652f53adf64f67632cbbd3ca5070fb965d35d6bc",
        about: "校验层，外加它要的 MSVC 运行库",
        origin: "LunarG 的校验层，library_path 改成反斜杠（Windows 的 LoadLibraryEx 不吃正斜杠），外加 https://aka.ms/vs/17/release/vc_redist.x64.exe 里的 MSVCP140.dll / VCRUNTIME140.dll / VCRUNTIME140_1.dll —— 那个层是 MSVC 编的，要这三个才起得来",
    },
];

/// 本机这个开发平台要装的全部条目。
pub fn entries(host: Host) -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(move |e| e.host == host)
}
