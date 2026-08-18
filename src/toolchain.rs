//! 工具链探测。
//!
//! 安装脚本只装 vkx 本身，其余全部来自 `vkx fetch` 取下来的 SDK 包。
//! 这里只负责找到它们，找不到就报错，让用户 `vkx fetch`。
//!
//! ~/.vkx 的布局。sdk/ 下面一个组件一个目录，和清单里的组件名一一对应——
//! fetch 就是按这个对应关系解包的，所以这里的路径必须跟着组件走：
//!
//!   bin/vkx
//!   sdk/toolchain/{cmake,ninja,slang,clang-format,llvm-mingw}
//!   sdk/vulkan/{vulkan,moltenvk}
//!   sdk/libs/{include,lib}                         预编译的 C 库和头文件
//!   sdk/sources/{jolt,gamenetworkingsockets}       要从源码编的那几个
//!   sdk/android/{jdk,gradle,sdk}                   移动端打包用（暂未进包）

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Code, Error, Result};

pub fn home_dir() -> PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 安装脚本铺好的环境根目录。
pub fn vkx_home() -> PathBuf {
    match std::env::var_os("VKX_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home_dir().join(".vkx"),
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// PATH 里找可执行文件。
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path) {
        for extension in &extensions {
            let candidate = dir.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 工具从哪儿来，分三类。
///
/// # 只用我们自己的（[`managed_only`]）
///
/// slangc、clang-format 这类**输出必须逐字节一致**的工具。教程里每一步的 diff
/// 和「校验层零输出」都建立在所有人拿到同样的产物上；读者机器上那个版本不同的
/// clang-format 会把 `vkx fmt` 的结果改掉，diff 就对不上了。
///
/// # 系统的够用就用系统的（[`system_or_managed`]）
///
/// cmake、ninja 这类只要版本够新、行为就一致的工具。用系统已有的能省一次下载。
/// 版本太旧则回退到我们装的那份。
///
/// # 只能用系统的
///
/// xcodebuild、xcrun、codesign、hdiutil——Apple 的东西不允许第三方再分发，
/// 只能指望机器上装了 Xcode。这是文档里写明的两个例外之一。
///
/// # 永远不用的
///
/// MSVC。就算机器上装了 Visual Studio 也不碰它：我们发的预编译库是 llvm-mingw
/// 编的，两种 ABI 混在一起会在链接期炸。
fn managed_only(relative: &str) -> Option<PathBuf> {
    let managed = vkx_home().join(relative);
    managed.is_file().then_some(managed)
}

/// 系统上那份够新就用它，否则用我们装的。`minimum` 是「主版本.次版本」。
fn system_or_managed(relative: &str, program: &str, minimum: (u32, u32)) -> Option<PathBuf> {
    if let Some(found) = which(program)
        && version_at_least(&found, minimum)
    {
        return Some(found);
    }
    let managed = vkx_home().join(relative);
    managed.is_file().then_some(managed)
}

/// 跑 `<程序> --version`，把第一个 `x.y` 抠出来比一下。认不出版本就当不满足。
fn version_at_least(program: &Path, minimum: (u32, u32)) -> bool {
    let Ok(output) = std::process::Command::new(program)
        .arg("--version")
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(found) = text.split_whitespace().find_map(|word| {
        let mut parts = word.split('.');
        let major: u32 = parts.next()?.parse().ok()?;
        let minor: u32 = parts.next().unwrap_or("0").parse().ok()?;
        Some((major, minor))
    }) else {
        return false;
    };
    found >= minimum
}

/// clang-format。只用 vkx 装的那份——版本不同格式化结果就不同，
/// 而教程的每一步 diff 都假设所有人格式化出来一模一样。
pub fn clang_format() -> Result<PathBuf> {
    let relative = format!("sdk/toolchain/clang-format/{}", exe("clang-format"));
    managed_only(&relative).ok_or_else(|| missing("clang-format", &relative))
}

fn missing(tool: &str, expected: &str) -> Error {
    Error::new(
        Code::MissingComponent,
        format!("找不到 {tool}"),
        format!("它应该在 {}", vkx_home().join(expected).display()),
    )
    .hint("执行 `vkx fetch` 把 SDK 组件补齐")
}

// ---------------------------------------------------------------------------
// 外部命令
// ---------------------------------------------------------------------------

/// 跑一个命令，输出直接透传给用户；失败时把命令行完整报出来。
pub fn run(command: &mut Command, what: &str) -> Result<()> {
    let rendered = render(command);
    let status = command.status().map_err(|e| {
        Error::new(
            Code::MissingComponent,
            format!("无法执行 {what}: {e}"),
            format!("命令: {rendered}"),
        )
    })?;
    if !status.success() {
        return Err(Error::new(
            Code::MissingComponent,
            format!("{what}失败（退出码 {}）", status.code().unwrap_or(-1)),
            format!("命令: {rendered}"),
        ));
    }
    Ok(())
}

/// 跑一个命令并捕获 stdout（探测版本号这类场景）。
pub fn capture(program: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn render(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    for arg in command.get_args() {
        parts.push(arg.to_string_lossy().to_string());
    }
    parts.join(" ")
}

// ---------------------------------------------------------------------------
// 构建工具
// ---------------------------------------------------------------------------

pub fn find_cmake() -> Option<PathBuf> {
    system_or_managed(
        &format!("sdk/toolchain/cmake/bin/{}", exe("cmake")),
        "cmake",
        (3, 24),
    )
}

pub fn require_cmake() -> Result<PathBuf> {
    find_cmake().ok_or_else(|| missing("cmake", "sdk/toolchain/cmake"))
}

pub fn find_ninja() -> Option<PathBuf> {
    system_or_managed(
        &format!("sdk/toolchain/ninja/{}", exe("ninja")),
        "ninja",
        (1, 10),
    )
}

pub fn require_ninja() -> Result<PathBuf> {
    find_ninja().ok_or_else(|| missing("ninja", "sdk/toolchain/ninja"))
}

pub fn find_slangc() -> Option<PathBuf> {
    let managed = vkx_home().join(format!("sdk/toolchain/slang/bin/{}", exe("slangc")));
    if managed.is_file() {
        return Some(managed);
    }
    // 装了 Vulkan SDK 的机器上也有一份。
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        for bin in ["bin", "Bin"] {
            let candidate = PathBuf::from(&sdk).join(bin).join(exe("slangc"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    which("slangc")
}

pub fn require_slangc() -> Result<PathBuf> {
    find_slangc().ok_or_else(|| missing("slangc（Slang 着色器编译器）", "sdk/toolchain/slang"))
}

/// llvm-mingw 里的 clang，Windows 上没有 MSVC 时用它。
pub fn llvm_mingw() -> Option<PathBuf> {
    let dir = vkx_home().join("sdk/toolchain/llvm-mingw");
    dir.join("bin").join(exe("clang")).is_file().then_some(dir)
}

// ---------------------------------------------------------------------------
// 依赖源码（构建时离线取用）
// ---------------------------------------------------------------------------

/// 返回 ~/.vkx/src/<name>，不存在则返回 None（届时 CMake 会自己去联网拉）。
pub fn source_dir(name: &str) -> Option<PathBuf> {
    let dir = vkx_home().join("src").join(name);
    dir.is_dir().then_some(dir)
}

/// SDL3 的 Android .aar，供 Gradle 的 prefab 使用。
pub fn sdl_android_aar() -> Result<PathBuf> {
    let dir = vkx_home().join("src/sdl3-android");
    let found = crate::fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .into_iter()
            .find(|path| path.extension().is_some_and(|ext| ext == "aar"))
    });
    found.ok_or_else(|| missing("SDL3 的 Android aar", "src/sdl3-android"))
}

// ---------------------------------------------------------------------------
// Java / Gradle / Android
// ---------------------------------------------------------------------------

pub fn jdk() -> Option<PathBuf> {
    let managed = vkx_home().join("sdk/android/jdk");
    if managed.join("bin").join(exe("java")).is_file() {
        return Some(managed);
    }
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let path = PathBuf::from(java_home);
        if path.join("bin").join(exe("java")).is_file() {
            return Some(path);
        }
    }
    // 系统 PATH 上的 java，往上两级就是 JAVA_HOME。
    which("java")?.parent()?.parent().map(PathBuf::from)
}

pub fn require_jdk() -> Result<PathBuf> {
    jdk().ok_or_else(|| missing("JDK", "sdk/android/jdk"))
}

pub fn keytool() -> Result<PathBuf> {
    let path = require_jdk()?.join("bin").join(exe("keytool"));
    if !path.is_file() {
        return Err(missing("keytool", "sdk/android/jdk/bin"));
    }
    Ok(path)
}

pub fn find_gradle() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "gradle.bat"
    } else {
        "gradle"
    };
    let managed = vkx_home().join("sdk/android/gradle/bin").join(name);
    if managed.is_file() {
        return Some(managed);
    }
    which("gradle")
}

pub fn require_gradle() -> Result<PathBuf> {
    find_gradle().ok_or_else(|| missing("gradle", "sdk/android/gradle"))
}

pub fn android_sdk() -> Option<PathBuf> {
    let managed = vkx_home().join("sdk/android/sdk");
    if managed.is_dir() {
        return Some(managed);
    }
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(value) = std::env::var_os(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    let default = if cfg!(target_os = "macos") {
        home_dir().join("Library/Android/sdk")
    } else if cfg!(windows) {
        home_dir().join("AppData/Local/Android/Sdk")
    } else {
        home_dir().join("Android/Sdk")
    };
    default.is_dir().then_some(default)
}

pub fn require_android_sdk() -> Result<PathBuf> {
    android_sdk().ok_or_else(|| missing("Android SDK", "sdk/android/sdk"))
}

/// SDK 下可能并存多个 NDK 版本，取版本号最大的。
pub fn android_ndk() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("ANDROID_NDK_HOME") {
        let path = PathBuf::from(value);
        if path.is_dir() {
            return Some(path);
        }
    }
    let ndk_root = android_sdk()?.join("ndk");
    let mut versions: Vec<PathBuf> = crate::fs::read_dir(&ndk_root)
        .ok()?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    versions.pop()
}

pub fn require_android_ndk() -> Result<PathBuf> {
    android_ndk().ok_or_else(|| missing("Android NDK", "sdk/android/sdk/ndk"))
}

pub fn adb() -> Option<PathBuf> {
    if let Some(sdk) = android_sdk() {
        let candidate = sdk.join("platform-tools").join(exe("adb"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    which("adb")
}

// ---------------------------------------------------------------------------
// Apple 平台
// ---------------------------------------------------------------------------

/// iOS 用的静态 MoltenVK。`device` 为真取真机切片，否则取模拟器切片。
pub fn moltenvk_lib(device: bool) -> Result<PathBuf> {
    let xcframework = vkx_home().join("sdk/vulkan/moltenvk/static/MoltenVK.xcframework");
    if !xcframework.is_dir() {
        return Err(missing("MoltenVK", "sdk/vulkan/moltenvk"));
    }
    let slice = if device {
        "ios-arm64"
    } else {
        "ios-arm64_x86_64-simulator"
    };
    let library = xcframework.join(slice).join("libMoltenVK.a");
    if !library.is_file() {
        return Err(Error::new(
            Code::MissingComponent,
            format!("MoltenVK 里没有 {slice} 这个切片"),
            "镜像上的 MoltenVK 包可能不完整，重新同步后再装一次",
        ));
    }
    Ok(library)
}

/// macOS 上运行游戏时要用到的 Vulkan 实现（MoltenVK 的动态库目录）。
pub fn moltenvk_dylib_dir() -> Option<PathBuf> {
    let dir = vkx_home().join("sdk/vulkan/moltenvk/dylib/macOS");
    dir.join("libMoltenVK.dylib").is_file().then_some(dir)
}

// ---------------------------------------------------------------------------
// 运行期
// ---------------------------------------------------------------------------

/// 启动游戏前要补的环境变量。
///
/// SDK 里带了 Vulkan 的 loader、ICD 和校验层，但它们在 ~/.vkx 下，不在系统的
/// 搜索路径上——不指过去，程序起来就是「找不到 Vulkan 运行时」，哪怕东西就在
/// 硬盘上躺着。
///
/// macOS 上不能靠 DYLD_LIBRARY_PATH：SIP 会在 /usr/bin/env、/bin/sh 这些受保护
/// 的二进制 exec 时把 DYLD_* 全部剥掉，路上随便经过一层就没了。改成用 SDL 的
/// SDL_VULKAN_LIBRARY 直接给绝对路径，跟中间经过谁无关。
///
/// 这一点是有区别的：走不到 loader 就没有层，程序照样能跑，只是校验层静悄悄
/// 地不见了——而这一章从头到尾都在教「以校验层的报错为准」。
///
/// 返回的是「要追加的值」，调用方负责和进程里已有的值拼起来：读者机器上可能
/// 装了别的 Vulkan SDK，我们只往前面插，不覆盖。
pub fn vulkan_runtime_env() -> Vec<(&'static str, PathBuf)> {
    let vulkan = vkx_home().join("sdk/vulkan/vulkan");
    if !vulkan.is_dir() {
        return Vec::new();
    }
    let mut env = Vec::new();
    let lib = vulkan.join("lib");

    if cfg!(target_os = "macos") {
        // macOS 上系统不带 loader，只能用 SDK 里这份。
        let loader = lib.join("libvulkan.1.dylib");
        if loader.is_file() {
            env.push(("SDL_VULKAN_LIBRARY", loader));
        }
        // ICD 也要指：Windows 和 Linux 的显卡驱动会自己注册，macOS 没人注册。
        let icd = vulkan.join("share/vulkan/icd.d/MoltenVK_icd.json");
        if icd.is_file() {
            env.push(("VK_ICD_FILENAMES", icd));
        }
    } else if lib.is_dir() {
        // 这两个平台的 loader 由驱动提供，我们只补校验层的动态库目录。
        let key = if cfg!(windows) {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        env.push((key, lib));
    }

    // 校验层三个平台都要指——它不在任何标准位置上。
    let layers = vulkan.join("share/vulkan/explicit_layer.d");
    if layers.is_dir() {
        env.push(("VK_LAYER_PATH", layers));
    }
    env
}

pub fn xcodebuild() -> Option<PathBuf> {
    which("xcodebuild")
}

/// macOS 上的 Xcode 命令行工具，由用户自己安装（Apple 的 SDK 不允许再分发）。
pub fn xcode_developer_dir() -> Option<String> {
    let tool = which("xcode-select")?;
    let dir = capture(&tool, &["-p"])?;
    Path::new(&dir).is_dir().then_some(dir)
}
