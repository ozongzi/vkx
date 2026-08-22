//! 工具链探测。
//!
//! 安装包里除了 vkx 本身，还带着全套 SDK 组件。这里只负责在 ~/.vkx 下找到
//! 它们，找不到就报错，让用户 `vkx doctor` 看缺什么、`vkx install` 补齐。
//!
//! ~/.vkx 的布局。sdk/ 下面一个组件一个目录，和清单里的组件名一一对应——
//! fetch 就是按这个对应关系解包的，所以这里的路径必须跟着组件走：
//!
//!   bin/vkx
//!   sdk/toolchain/{cmake,ninja,slang,clang-format,llvm-mingw}
//!   sdk/vulkan/{vulkan,moltenvk}
//!   sdk/libs/<target>/{include,lib}                预编译库，一个 target 一份
//!   sdk/maven/                                  安卓构建要的 Gradle 依赖
//!   sdk/sdl3-android/                          SDL3 的安卓 .aar
//!   sdk/android/{jdk,gradle,sdk}                   移动端打包用（暂未进包）

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Code, Error, Result};

/// 用户主目录。找不到就退回当前目录——但会先喊一声。
///
/// 静默退回当前目录是个很不好查的坑：`vkx install` 会把几百 MB 装到你随手所在
/// 的那个目录里，而且报告成功，直到后面某一步说「找不到 cmake」才暴露。
pub fn home_dir() -> PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    match std::env::var_os(key) {
        Some(dir) => PathBuf::from(dir),
        None => {
            crate::ui::warn(&format!(
                "环境变量 {key} 没有设置，只能把 ~/.vkx 当成当前目录下的 .vkx。\n\
                 要装到别处就设 VKX_HOME。"
            ));
            PathBuf::from(".")
        }
    }
}

/// 把一个路径变成 CMake 能安全吃下的字符串。只有 Windows 上才真的动手。
///
/// 两件事：
///
/// 一是去掉 `\\?\` 前缀。`Path::canonicalize()` 在 Windows 上返回的一律是这种
/// 「扩展长度路径」，而 CMake 不认——它把分隔符统一成正斜杠之后得到 `//?/D:/x`，
/// 会当成网络路径去解析，编译器探测和生成的 CMakeCCompiler.cmake 就全是坏的。
///
/// 二是反斜杠换正斜杠。CMake 的字符串里反斜杠是转义符，`C:\Users\...` 里的
/// `\U` 是非法转义，include 生成的 .cmake 文件时会直接报语法错误。正斜杠 CMake
/// 在 Windows 上照样认，而且没有这个歧义。
pub fn cmake_path(path: &Path) -> String {
    let text = path.display().to_string();
    if cfg!(windows) {
        windows_cmake_text(&text)
    } else {
        // Unix 的文件名里反斜杠是合法字符，不能乱换。
        text
    }
}

/// 把路径统一成当前平台的原生分隔符。Windows 上就是全部换成反斜杠。
///
/// 起因是 Windows 的 `LoadLibraryEx`：带 `LOAD_LIBRARY_SEARCH_*` 标志时它**不接受
/// 正斜杠**，直接返回 ERROR_INVALID_PARAMETER(87)。而 Vulkan 加载器正是这么调的，
/// 于是 `VK_LAYER_PATH` 里只要混进一个正斜杠，校验层就加载不了——报出来还是
/// 一句和路径毫无关系的 `VK_ERROR_OUT_OF_HOST_MEMORY`。
///
/// 混合分隔符很容易出现：`vkx_home()` 来自 USERPROFILE（反斜杠），后面
/// `.join("sdk/vulkan/vulkan")` 拼的是字面量（正斜杠），拼出来就是
/// `C:\Users\me\.vkx\sdk/vulkan/vulkan`。Windows API 大多不在乎，偏偏这个在乎。
pub fn native_path(path: &Path) -> String {
    let text = path.display().to_string();
    if cfg!(windows) {
        windows_native_text(&text)
    } else {
        text
    }
}

/// 摘前缀 + 正斜杠换反斜杠。
fn windows_native_text(text: &str) -> String {
    strip_verbatim(text).replace('/', "\\")
}

/// 摘掉 Windows 的 `\\?\` 扩展长度前缀，别的原样返回。
///
/// `Path::canonicalize()` 在 Windows 上一律返回这种前缀。它对 Win32 API 有用
/// （能突破 260 字符上限），但 CMake、ninja 这类工具多半不认，`D:\client` 这种
/// 短路径又根本用不着它。
pub fn plain_path(path: &Path) -> PathBuf {
    if !cfg!(windows) {
        return path.to_path_buf();
    }
    PathBuf::from(strip_verbatim(&path.display().to_string()))
}

// 下面两个是纯字符串函数，不看当前平台。这样 Windows 的路径处理在 mac 和 Linux
// 上也能被 `cargo test` 覆盖到——这条路径的真机验证要经过 CI 出二进制、再在
// Windows 上手动换掉 ~/.vkx/bin/vkx，一轮很贵，靠单测把它钉住划算得多。

/// `\\?\D:\x` -> `D:\x`，`\\?\UNC\server\share` -> `\\server\share`。
fn strip_verbatim(text: &str) -> String {
    // UNC 那种要还原成 \\server\share，直接砍前缀会砍出个半截路径。
    match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => match text.strip_prefix(r"\\?\") {
            Some(rest) => rest.to_string(),
            None => text.to_string(),
        },
    }
}

/// 摘前缀 + 反斜杠换正斜杠。
fn windows_cmake_text(text: &str) -> String {
    strip_verbatim(text).replace('\\', "/")
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
/// # 只用我们自己装的（[`managed_only`]）
///
/// cmake、ninja、slangc、clang-format、llvm-mingw、JDK、Gradle、Android SDK/NDK
/// ——凡是安装包里带的，一律只认 `~/.vkx` 下的那一份，*不看 PATH，也不看
/// VULKAN_SDK / JAVA_HOME / ANDROID_HOME 这些环境变量*。
///
/// 理由是可复现。教程里每一步的 diff、「校验层零输出」、`vkx fmt` 的结果，都建立
/// 在所有人拿到同一套工具上。一旦允许回退到系统那份，读者的构建就取决于他机器上
/// 恰好装了什么——PATH 上某个别的软件捎带的 cmake 就能让构建走上另一条路，而且
/// 出了问题极难判断：报错里根本看不出用的是哪一份。
///
/// 少下一份的那点磁盘，换不来这个不确定性。装不全就报错让人去补齐，
/// 比默默用一份来路不明的强。
///
/// # 只能用系统的
///
/// xcodebuild、xcrun、codesign、hdiutil——Apple 的东西不允许第三方再分发，
/// 只能指望机器上装了 Xcode。这是唯一的例外，也是 [`which`] 仅剩的用处。
///
/// # 永远不用的
///
/// MSVC。就算机器上装了 Visual Studio 也不碰它：我们发的预编译库是 llvm-mingw
/// 编的，两种 ABI 混在一起会在链接期炸。
fn managed_only(relative: &str) -> Option<PathBuf> {
    let managed = vkx_home().join(relative);
    managed.is_file().then_some(managed)
}

/// 同上，只不过找的是目录。
fn managed_dir(relative: &str) -> Option<PathBuf> {
    let managed = vkx_home().join(relative);
    managed.is_dir().then_some(managed)
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
    .hint("`vkx doctor` 看缺了哪些，再用 `vkx install vkx-<平台>.zip` 补齐")
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
    managed_only(&format!("sdk/toolchain/cmake/bin/{}", exe("cmake")))
}

pub fn require_cmake() -> Result<PathBuf> {
    find_cmake().ok_or_else(|| missing("cmake", "sdk/toolchain/cmake"))
}

pub fn find_ninja() -> Option<PathBuf> {
    managed_only(&format!("sdk/toolchain/ninja/{}", exe("ninja")))
}

pub fn require_ninja() -> Result<PathBuf> {
    find_ninja().ok_or_else(|| missing("ninja", "sdk/toolchain/ninja"))
}

pub fn find_slangc() -> Option<PathBuf> {
    // 装了 Vulkan SDK 的机器上也有一份 slangc，但版本不一定对得上：换个版本
    // 产出的 SPIR-V 就可能不一样，不认。
    managed_only(&format!("sdk/toolchain/slang/bin/{}", exe("slangc")))
}

pub fn require_slangc() -> Result<PathBuf> {
    find_slangc().ok_or_else(|| missing("slangc（Slang 着色器编译器）", "sdk/toolchain/slang"))
}

/// llvm-mingw 里的 clang，Windows 上没有 MSVC 时用它。
/// Linux 上的 C++ 编译器，SDK 自带。
///
/// 和 Windows 用 llvm-mingw 是同一个理由，只是 Linux 上更严重：这里发行版默认
/// 连 g++ 都不装（实测 Ubuntu 24.04 桌面版一个 C++ 编译器都没有），而就算学员
/// 自己装了 clang，Linux 上的 clang 默认仍然用系统的 libstdc++——版本和
/// _GLIBCXX_USE_CXX11_ABI 各机器不同，预编译的 C++ 库照样对不上。
///
/// 所以这里连 libc++ 一起自带，并且静态链进产物：钉死的必须是运行时，不只是编译器。
pub fn llvm_linux() -> Option<PathBuf> {
    let dir = vkx_home().join("sdk/toolchain/llvm");
    dir.join("bin").join(exe("clang")).is_file().then_some(dir)
}

pub fn llvm_mingw() -> Option<PathBuf> {
    let dir = vkx_home().join("sdk/toolchain/llvm-mingw");
    dir.join("bin").join(exe("clang")).is_file().then_some(dir)
}

// ---------------------------------------------------------------------------
// 依赖源码（构建时离线取用）
// ---------------------------------------------------------------------------

/// SDL3 的 Android .aar，供 Gradle 的 prefab 使用。
/// 安卓构建要的 Gradle 依赖仓库（AGP 及其闭包），随离线包分发。
pub fn maven_repo() -> Option<PathBuf> {
    let dir = vkx_home().join("sdk/maven");
    dir.is_dir().then_some(dir)
}

pub fn sdl_android_aar() -> Result<PathBuf> {
    let dir = vkx_home().join("sdk/sdl3-android");
    let found = crate::fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .into_iter()
            .find(|path| path.extension().is_some_and(|ext| ext == "aar"))
    });
    found.ok_or_else(|| missing("SDL3 的 Android aar", "sdk/sdl3-android"))
}

// ---------------------------------------------------------------------------
// Java / Gradle / Android
// ---------------------------------------------------------------------------

pub fn jdk() -> Option<PathBuf> {
    // 不看 JAVA_HOME，也不看 PATH 上的 java：Gradle 对 JDK 版本很挑，
    // 机器上是几就用几的话，安卓构建会随人而异。
    let managed = vkx_home().join("sdk/android/jdk");
    managed
        .join("bin")
        .join(exe("java"))
        .is_file()
        .then_some(managed)
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
    managed.is_file().then_some(managed)
}

pub fn require_gradle() -> Result<PathBuf> {
    find_gradle().ok_or_else(|| missing("gradle", "sdk/android/gradle"))
}

pub fn android_sdk() -> Option<PathBuf> {
    // 不看 ANDROID_HOME / ANDROID_SDK_ROOT，也不去猜 ~/Library/Android/sdk 那些
    // 默认位置：Android Studio 装的那套里 NDK、build-tools 版本都不确定。
    managed_dir("sdk/android/sdk")
}

pub fn require_android_sdk() -> Result<PathBuf> {
    android_sdk().ok_or_else(|| missing("Android SDK", "sdk/android/sdk"))
}

/// SDK 下可能并存多个 NDK 版本，取版本号最大的。
pub fn android_ndk() -> Option<PathBuf> {
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
    let candidate = android_sdk()?.join("platform-tools").join(exe("adb"));
    candidate.is_file().then_some(candidate)
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
        // 两个名字都给：VK_DRIVER_FILES 是现在的名字，VK_ICD_FILENAMES 是
        // 旧名，读者机器上的 loader 版本不确定，给两个不冲突。
        let icd = vulkan.join("share/vulkan/icd.d/MoltenVK_icd.json");
        if icd.is_file() {
            env.push(("VK_DRIVER_FILES", icd.clone()));
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

#[cfg(test)]
mod tests {
    use super::{strip_verbatim, windows_cmake_text, windows_native_text};

    #[test]
    fn 扩展长度前缀被摘掉() {
        assert_eq!(strip_verbatim(r"\\?\D:\client\target"), r"D:\client\target");
        // UNC 要还原成 \\server\share，不能砍成 server\share
        assert_eq!(strip_verbatim(r"\\?\UNC\nas\share\x"), r"\\nas\share\x");
        // 没有前缀的原样返回
        assert_eq!(strip_verbatim(r"D:\client"), r"D:\client");
        assert_eq!(strip_verbatim("/Users/me/.vkx"), "/Users/me/.vkx");
    }

    #[test]
    fn 交给加载器的路径没有正斜杠() {
        // Windows 的 LoadLibraryEx 带 LOAD_LIBRARY_SEARCH_* 时正斜杠会报 87，
        // 而 vkx_home() 是反斜杠、join 的字面量是正斜杠，天然会混。
        assert_eq!(
            windows_native_text(
                r"C:\Users\me\.vkx\sdk/vulkan/vulkan\share/vulkan/explicit_layer.d"
            ),
            r"C:\Users\me\.vkx\sdk\vulkan\vulkan\share\vulkan\explicit_layer.d"
        );
        assert!(!windows_native_text(r"C:\Users\me\.vkx\sdk/vulkan").contains('/'));
    }

    #[test]
    fn 交给_cmake_的路径没有反斜杠() {
        // -S / -B：带前缀的话 CMake 会看到 //?/D:/... 当成网络路径
        assert_eq!(
            windows_cmake_text(r"\\?\D:\client\target\debug"),
            "D:/client/target/debug"
        );
        // 这条就是线上炸掉的那个：\Users 的 \U 是 CMake 里的非法转义。
        // 注意源串是斜杠混着的——PathBuf::join 拼字面量就会拼成这样。
        assert_eq!(
            windows_cmake_text(r"C:\Users\me\.vkx\sdk/toolchain/llvm-mingw\bin\clang.exe"),
            "C:/Users/me/.vkx/sdk/toolchain/llvm-mingw/bin/clang.exe"
        );
        assert!(!windows_cmake_text(r"C:\Users\x\.vkx").contains('\\'));
    }
}
