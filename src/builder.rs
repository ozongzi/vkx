use std::path::PathBuf;
use std::process::Command;

use crate::error::{Code, Error, Result};
use crate::project::Project;
use crate::toolchain;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
    Release,
}

impl Profile {
    pub fn dir(self) -> &'static str {
        match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    }

    pub fn cmake_config(self) -> &'static str {
        match self {
            Profile::Debug => "Debug",
            Profile::Release => "Release",
        }
    }
}

/// 给 CMake 配置命令补上依赖源码的位置。
///
/// 安装脚本已经把 SDL3 等源码放在 ~/.vkx/src 下，用 FetchContent 的
/// FETCHCONTENT_SOURCE_DIR_<名字> 指过去，构建全程不用联网。
pub fn add_offline_sources(command: &mut Command) {
    for (name, variable) in [
        ("sdl3", "FETCHCONTENT_SOURCE_DIR_SDL3"),
        ("vulkan-headers", "FETCHCONTENT_SOURCE_DIR_VULKANHEADERS"),
        ("volk", "FETCHCONTENT_SOURCE_DIR_VOLK"),
    ] {
        if let Some(dir) = toolchain::source_dir(name) {
            command.arg(format!("-D{variable}={}", dir.display()));
        }
    }
}

/// 挑生成器和编译器。
///
/// 一律 Ninja + llvm-mingw（Windows）。
///
/// 就算机器上装了 Visual Studio 也不用 MSVC：SDK 包里的预编译库是 llvm-mingw
/// 编的，两种 ABI 混在一起会在链接期炸。MSVC 的工具集也不允许我们分发，
/// 用它就等于让每个读者的产物取决于自己装了哪个版本的 VS。
fn add_generator(command: &mut Command) -> Result<()> {
    let ninja = toolchain::require_ninja()?;
    command
        .arg("-G")
        .arg("Ninja")
        .arg(format!("-DCMAKE_MAKE_PROGRAM={}", ninja.display()));

    if cfg!(windows) {
        let mingw = toolchain::llvm_mingw().ok_or_else(|| {
            Error::new(
                Code::CommandFailed,
                "Windows 上没有 llvm-mingw",
                "重新运行安装脚本，它会装一份 llvm-mingw",
            )
        })?;
        let bin = mingw.join("bin");
        command
            .arg(format!(
                "-DCMAKE_C_COMPILER={}",
                bin.join("clang.exe").display()
            ))
            .arg(format!(
                "-DCMAKE_CXX_COMPILER={}",
                bin.join("clang++.exe").display()
            ))
            // 静态链接运行时，产物不依赖 llvm-mingw 的 DLL。
            .arg("-DCMAKE_EXE_LINKER_FLAGS=-static");
    }
    Ok(())
}

/// 确认本机有能用的 C++ 工具链。
///
/// 交给 CMake 去发现的话，报错是一句「No CMAKE_CXX_COMPILER could be found」，
/// 看不出该装什么，所以在这里先拦一道。
fn check_cxx_toolchain() -> Result<()> {
    if cfg!(target_os = "macos") && toolchain::xcode_developer_dir().is_none() {
        return Err(Error::new(
            Code::CommandFailed,
            "找不到 Xcode 命令行工具，编译 C++ 需要它",
            "执行：xcode-select --install",
        )
        .hint("Apple 的 SDK 不允许第三方分发，这是唯一要你手动装的东西"));
    }
    if cfg!(target_os = "linux") && !std::path::Path::new("/usr/include/stdio.h").exists() {
        return Err(Error::new(
            Code::CommandFailed,
            "缺少 libc 的开发头文件，链接会失败",
            "Debian/Ubuntu：sudo apt install build-essential",
        )
        .hint("Fedora：sudo dnf install gcc-c++ glibc-devel"));
    }
    Ok(())
}

/// 配置 + 编译桌面版，返回可执行文件路径。
/// 缺哪个 SDK 组件就取哪个。
///
/// 安装脚本只装 vkx 本身，组件是第一次构建时才下的——install.sh 结尾那句
/// 「第一次 vkx run 会下载编译需要的工具链」说的就是这里。以前这里不取，
/// 直接报「找不到 cmake，执行 vkx fetch」，多让人跑一条命令。
fn ensure_sdk(project: &Project) -> Result<()> {
    let mut needed = vec!["toolchain", "libs", "vulkan"];
    // 打开了要从源码编的库，就还要 sources。
    if !project.libs.is_empty() {
        needed.push("sources");
    }
    crate::fetch::ensure(&needed)
}

pub fn build(project: &Project, profile: Profile) -> Result<PathBuf> {
    ensure_sdk(project)?;
    check_cxx_toolchain()?;
    let cmake = toolchain::require_cmake()?;
    let slangc = toolchain::require_slangc()?;
    crate::generate::cmake(project)?;
    let build_dir = project.build_dir(profile.dir());

    ui::step(&format!(
        "配置 {} ({})",
        project.name,
        profile.cmake_config()
    ));
    let mut configure = Command::new(&cmake);
    configure
        .arg("-S")
        .arg(project.cmake_dir())
        .arg("-B")
        .arg(&build_dir)
        .arg(format!("-DCMAKE_BUILD_TYPE={}", profile.cmake_config()))
        .arg(format!("-DVKX_SLANGC={}", slangc.display()))
        .arg("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON");
    add_generator(&mut configure)?;
    add_offline_sources(&mut configure);

    toolchain::run(&mut configure, "CMake 配置").map_err(|e| {
        e.hint(format!(
            "上面是 CMake 的原始输出。删掉 {} 可以从头再来",
            build_dir.display()
        ))
    })?;

    ui::step("编译");
    let mut build = Command::new(&cmake);
    build
        .arg("--build")
        .arg(&build_dir)
        .arg("--config")
        .arg(profile.cmake_config())
        .arg("--parallel");
    toolchain::run(&mut build, "编译")?;

    locate_executable(project, profile)
}

fn locate_executable(project: &Project, profile: Profile) -> Result<PathBuf> {
    let build_dir = project.build_dir(profile.dir());
    let name = &project.name;
    // 单配置生成器（Ninja）直接放在根上，多配置生成器（MSVC）会多一层 Debug/。
    let candidates = [
        build_dir.join(name),
        build_dir.join(format!("{name}.exe")),
        build_dir.join(profile.cmake_config()).join(name),
        build_dir
            .join(profile.cmake_config())
            .join(format!("{name}.exe")),
    ];

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            Error::new(
                Code::CommandFailed,
                format!("编译完成，但在 {} 里没找到可执行文件", build_dir.display()),
                "如果你改过 CMakeLists.txt 里的 target 名字，vkx.toml 的 name 也要一起改",
            )
        })
}

/// 编译并运行；返回被运行程序的退出码。
pub fn run(project: &Project, profile: Profile, args: &[String]) -> Result<i32> {
    let executable = build(project, profile)?;

    ui::step(&format!("运行 {}", executable.display()));
    let mut command = Command::new(&executable);
    command.args(args).current_dir(&project.root);

    // Vulkan 的 loader / ICD / 校验层都在 SDK 里，但不在系统搜索路径上。
    // 不在这里指过去，程序起来就报「找不到 Vulkan 运行时」——东西明明就在
    // 硬盘上。往前面插，不覆盖：读者机器上可能另装了 Vulkan SDK。
    for (key, value) in toolchain::vulkan_runtime_env() {
        let merged = match std::env::var_os(key) {
            // 这两个是「就用这一个」，不是搜索路径，拼接反而会读不出来。
            Some(existing)
                if !existing.is_empty()
                    && key != "VK_ICD_FILENAMES"
                    && key != "SDL_VULKAN_LIBRARY" =>
            {
                let separator = if cfg!(windows) { ";" } else { ":" };
                format!(
                    "{}{separator}{}",
                    value.display(),
                    existing.to_string_lossy()
                )
            }
            _ => value.display().to_string(),
        };
        command.env(key, merged);
    }

    let status = command.status().map_err(|e| {
        Error::new(
            Code::CommandFailed,
            format!("无法启动 {}: {e}", executable.display()),
            "确认构建成功且产物有执行权限；`vkx build` 重新编一次",
        )
    })?;

    let code = status.code().unwrap_or(-1);
    if code != 0 {
        ui::warn(&format!("程序退出码 {code}"));
        ui::info("上面若有 Vulkan 相关报错，多半是显卡驱动缺少 Vulkan 支持。");
    }
    Ok(code)
}
