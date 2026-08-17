//! 工具链探测。
//!
//! vkx 自己不下载任何东西：环境由安装脚本（install.sh / install.ps1）从镜像
//! 铺到 ~/.vkx 下，这里只负责找到它们。找不到就报错，让用户重跑安装脚本。
//!
//! ~/.vkx 的布局：
//!   bin/vkx
//!   tools/{cmake,ninja,slang,jdk,gradle,moltenvk,llvm-mingw}
//!   android/sdk/{cmdline-tools,platform-tools,build-tools,platforms,ndk}
//!   src/{sdl3,sdl3-android,vulkan-headers,volk}     构建时离线取用的源码
//!   env.sh                                          给用户 shell 用的环境

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

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

/// 先看 vkx 装的那份，再退回系统 PATH 上的。
fn managed_or_system(relative: &str, program: &str) -> Option<PathBuf> {
    let managed = vkx_home().join(relative);
    if managed.is_file() {
        return Some(managed);
    }
    which(program)
}

/// clang-format。先用 vkx 装的那份，保证所有人格式化结果一致；
/// 没装（比如用了 --no-vkx）就退回系统 PATH 上的。
pub fn clang_format() -> Result<PathBuf> {
    let relative = format!("tools/clang-format/{}", exe("clang-format"));
    managed_or_system(&relative, "clang-format").ok_or_else(|| missing("clang-format", &relative))
}

fn missing(tool: &str, expected: &str) -> Error {
    Error::new(format!("找不到 {tool}"))
        .hint(format!(
            "安装脚本应该把它装在 {}",
            vkx_home().join(expected).display()
        ))
        .hint("重新运行安装脚本即可补齐")
}

// ---------------------------------------------------------------------------
// 外部命令
// ---------------------------------------------------------------------------

/// 跑一个命令，输出直接透传给用户；失败时把命令行完整报出来。
pub fn run(command: &mut Command, what: &str) -> Result<()> {
    let rendered = render(command);
    let status = command.status().map_err(|e| {
        Error::new(format!("无法执行 {what}: {e}")).hint(format!("命令: {rendered}"))
    })?;
    if !status.success() {
        return Err(Error::new(format!(
            "{what}失败（退出码 {}）",
            status.code().unwrap_or(-1)
        ))
        .hint(format!("命令: {rendered}")));
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
    managed_or_system(&format!("tools/cmake/bin/{}", exe("cmake")), "cmake")
}

pub fn require_cmake() -> Result<PathBuf> {
    find_cmake().ok_or_else(|| missing("cmake", "tools/cmake"))
}

pub fn find_ninja() -> Option<PathBuf> {
    managed_or_system(&format!("tools/ninja/{}", exe("ninja")), "ninja")
}

pub fn require_ninja() -> Result<PathBuf> {
    find_ninja().ok_or_else(|| missing("ninja", "tools/ninja"))
}

pub fn find_slangc() -> Option<PathBuf> {
    let managed = vkx_home().join(format!("tools/slang/bin/{}", exe("slangc")));
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
    find_slangc().ok_or_else(|| missing("slangc（Slang 着色器编译器）", "tools/slang"))
}

/// Windows 上是否装了 MSVC；装了就优先用它，没有则用 llvm-mingw。
pub fn windows_msvc() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let program_files = std::env::var_os("ProgramFiles(x86)")?;
    let vswhere =
        PathBuf::from(program_files).join("Microsoft Visual Studio/Installer/vswhere.exe");
    if !vswhere.is_file() {
        return None;
    }
    let output = capture(
        &vswhere,
        &[
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationVersion",
        ],
    )?;
    (!output.is_empty()).then_some(output)
}

/// llvm-mingw 里的 clang，Windows 上没有 MSVC 时用它。
pub fn llvm_mingw() -> Option<PathBuf> {
    let dir = vkx_home().join("tools/llvm-mingw");
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
    let found = std::fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "aar"))
    });
    found.ok_or_else(|| missing("SDL3 的 Android aar", "src/sdl3-android"))
}

// ---------------------------------------------------------------------------
// Java / Gradle / Android
// ---------------------------------------------------------------------------

pub fn jdk() -> Option<PathBuf> {
    let managed = vkx_home().join("tools/jdk");
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
    jdk().ok_or_else(|| missing("JDK", "tools/jdk"))
}

pub fn keytool() -> Result<PathBuf> {
    let path = require_jdk()?.join("bin").join(exe("keytool"));
    if !path.is_file() {
        return Err(missing("keytool", "tools/jdk/bin"));
    }
    Ok(path)
}

pub fn find_gradle() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "gradle.bat"
    } else {
        "gradle"
    };
    let managed = vkx_home().join("tools/gradle/bin").join(name);
    if managed.is_file() {
        return Some(managed);
    }
    which("gradle")
}

pub fn require_gradle() -> Result<PathBuf> {
    find_gradle().ok_or_else(|| missing("gradle", "tools/gradle"))
}

pub fn android_sdk() -> Option<PathBuf> {
    let managed = vkx_home().join("android/sdk");
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
    android_sdk().ok_or_else(|| missing("Android SDK", "android/sdk"))
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
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&ndk_root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    versions.pop()
}

pub fn require_android_ndk() -> Result<PathBuf> {
    android_ndk().ok_or_else(|| missing("Android NDK", "android/sdk/ndk"))
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
    let xcframework = vkx_home().join("tools/moltenvk/static/MoltenVK.xcframework");
    if !xcframework.is_dir() {
        return Err(missing("MoltenVK", "tools/moltenvk"));
    }
    let slice = if device {
        "ios-arm64"
    } else {
        "ios-arm64_x86_64-simulator"
    };
    let library = xcframework.join(slice).join("libMoltenVK.a");
    if !library.is_file() {
        return Err(Error::new(format!("MoltenVK 里没有 {slice} 这个切片"))
            .hint("镜像上的 MoltenVK 包可能不完整，重新同步后再装一次"));
    }
    Ok(library)
}

/// macOS 上运行游戏时要用到的 Vulkan 实现（MoltenVK 的动态库目录）。
pub fn moltenvk_dylib_dir() -> Option<PathBuf> {
    let dir = vkx_home().join("tools/moltenvk/dylib/macOS");
    dir.join("libMoltenVK.dylib").is_file().then_some(dir)
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
