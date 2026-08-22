use std::path::{Path, PathBuf};
use std::process::Command;

use crate::builder::Profile;
use crate::error::{Code, Error, Result};
use crate::project::Project;
use crate::signing;
use crate::toolchain;
use crate::ui;

// ===========================================================================
// Android
// ===========================================================================

/// 跑一个 Gradle 任务。前面那一大段是把环境准备齐：SDL 的 aar、签名密钥、
/// local.properties，以及要传给 CMake 的各种路径。
fn run_gradle_task(project: &Project, profile: Profile, task: &str) -> Result<PathBuf> {
    // Gradle 会去调 CMake，所以生成物要先就位。
    crate::generate::cmake(project)?;
    let android_dir = project.root.join("android");
    if !android_dir.is_dir() {
        return Err(Error::new(
            Code::Environment,
            "工程里没有 android/ 目录",
            "用新版 vkx new 生成的工程才带 Android 支持",
        ));
    }

    let sdk = toolchain::require_android_sdk()?;
    let ndk = toolchain::require_android_ndk()?;
    let jdk = toolchain::require_jdk()?;
    let gradle = toolchain::require_gradle()?;
    let slangc = toolchain::require_slangc()?;
    // Android Gradle Plugin 只认 PATH 上的 ninja。
    let ninja = toolchain::require_ninja()?;

    // SDL3 的 Android 版以官方 .aar 提供，放进 app/libs/ 后 Gradle 的 prefab
    // 会把头文件、libSDL3.so 和 SDLActivity 一起接进来。
    let libs = android_dir.join("app").join("libs");
    crate::fs::create_dir_all(&libs)?;
    let source_aar = toolchain::sdl_android_aar()?;
    let file_name = source_aar.file_name().unwrap_or_default();
    let target_aar = libs.join(file_name);
    if !target_aar.is_file() {
        crate::fs::copy(&source_aar, &target_aar)?;
    }

    // release 包要签名；密钥一般在 vkx new 时就生成好了，缺了就现在补。
    if profile == Profile::Release && !signing::is_configured(&project.root) {
        signing::ensure_keystore(&project.root, &project.package_id)?;
    }

    write_local_properties(&android_dir, &sdk)?;

    ui::step(&format!("Gradle {task}"));
    let mut command = Command::new(&gradle);
    command
        .current_dir(&android_dir)
        .arg(task)
        .arg(format!("-PvkxSlangc={}", slangc.display()))
        .arg(format!("-PvkxNdkPath={}", ndk.display()))
        .env("JAVA_HOME", &jdk)
        .env("ANDROID_HOME", &sdk)
        .env("ANDROID_NDK_HOME", &ndk)
        .env(
            "PATH",
            prepend_to_path(ninja.parent().unwrap_or(Path::new("."))),
        );

    // Gradle 的依赖也在包里，指过去并让它离线跑：--offline 之后 Gradle 一旦
    // 需要联网就会直接失败，而不是悄悄去 google() 拉一份回来。
    if let Some(dir) = toolchain::maven_repo() {
        command
            .arg(format!("-PvkxMaven={}", dir.display()))
            .arg("--offline");
    }

    toolchain::run(&mut command, "Gradle 构建")?;
    Ok(android_dir)
}

fn flavor_of(profile: Profile) -> &'static str {
    match profile {
        Profile::Debug => "debug",
        Profile::Release => "release",
    }
}

/// 出 APK。
pub fn build_android(project: &Project, profile: Profile) -> Result<PathBuf> {
    // 桌面那三个组件之外还要 android（JDK / Gradle / SDK / NDK）。它有 1.3 GB，
    // 所以只在真的要出 APK 时才取——桌面读者的 vkx build 碰都不会碰到它。

    let flavor = flavor_of(profile);
    let task = match profile {
        Profile::Debug => "assembleDebug",
        Profile::Release => "assembleRelease",
    };
    let android_dir = run_gradle_task(project, profile, task)?;
    let output_dir = android_dir.join("app/build/outputs/apk").join(flavor);

    // 没有签名配置时，AGP 会把 release 包命名成 app-release-unsigned.apk。
    let candidates = [
        output_dir.join(format!("app-{flavor}.apk")),
        output_dir.join(format!("app-{flavor}-unsigned.apk")),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            Error::new(
                Code::CommandFailed,
                format!("Gradle 跑完了，但 {} 里没有 apk", output_dir.display()),
                "看上面 Gradle 的输出找真正的失败原因",
            )
        })
}

/// 出 AAB（Android App Bundle），上架 Google Play 用的格式。
pub fn bundle_android(project: &Project) -> Result<PathBuf> {
    let android_dir = run_gradle_task(project, Profile::Release, "bundleRelease")?;
    let aab = android_dir.join("app/build/outputs/bundle/release/app-release.aab");
    if !aab.is_file() {
        return Err(Error::new(
            Code::CommandFailed,
            format!("Gradle 跑完了，但没找到 {}", aab.display()),
            "看上面 Gradle 的输出找真正的失败原因",
        ));
    }
    Ok(aab)
}

pub fn run_android(project: &Project, profile: Profile) -> Result<i32> {
    let apk = build_android(project, profile)?;

    if apk.to_string_lossy().contains("-unsigned") {
        return Err(Error::new(
            Code::Environment,
            format!("{} 没有签名，无法安装到设备", apk.display()),
            "检查 android/keystore.properties 是否存在（vkx new 时会生成）",
        ));
    }

    let adb = toolchain::adb().ok_or_else(|| {
        Error::new(
            Code::Environment,
            "找不到 adb",
            "安装脚本会把它装在 ~/.vkx/android/sdk/platform-tools",
        )
    })?;

    ensure_device_connected(&adb)?;

    ui::step("安装到设备");
    toolchain::run(
        Command::new(&adb).arg("install").arg("-r").arg(&apk),
        "adb install",
    )?;

    ui::step("启动");
    let activity = format!("{}/{}.MainActivity", project.package_id, project.package_id);
    let _ = Command::new(&adb).args(["logcat", "-c"]).status();
    toolchain::run(
        Command::new(&adb).args(["shell", "am", "start", "-n", &activity]),
        "启动 Activity",
    )?;

    ui::info("下面是设备日志（Ctrl-C 退出）：");
    let _ = Command::new(&adb)
        .args([
            "logcat",
            "-s",
            "SDL",
            "SDL/APP",
            "vkx",
            "AndroidRuntime",
            "DEBUG",
        ])
        .status();
    Ok(0)
}

fn ensure_device_connected(adb: &Path) -> Result<()> {
    let listing = toolchain::capture(adb, &["devices"]).unwrap_or_default();
    let connected = listing
        .lines()
        .skip(1)
        .any(|line| line.trim_end().ends_with("device"));
    if connected {
        return Ok(());
    }
    Err(Error::new(
        Code::Environment,
        "没有已连接的 Android 设备",
        "真机：打开「开发者选项 → USB 调试」并允许本机调试",
    )
    .hint("模拟器：先在 Android Studio 的 Device Manager 里启动一个 AVD")
    .hint("确认连上了可以跑：adb devices"))
}

fn prepend_to_path(directory: &Path) -> std::ffi::OsString {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut entries = vec![directory.to_path_buf()];
    entries.extend(std::env::split_paths(&current));
    std::env::join_paths(entries).unwrap_or(current)
}

fn write_local_properties(android_dir: &Path, sdk: &Path) -> Result<()> {
    // Gradle 读 local.properties 找 SDK；路径里的反斜杠要转义（Windows）。
    let escape = |path: &Path| path.display().to_string().replace('\\', "\\\\");
    let content = format!("# 由 vkx 生成，不要提交进版本库\nsdk.dir={}\n", escape(sdk));
    crate::fs::write(&android_dir.join("local.properties"), content)?;
    Ok(())
}

// ===========================================================================
// iOS
// ===========================================================================

/// 生成 Xcode 工程（不编译），返回 .xcodeproj 的路径。
///
/// 命令行构建和在 Xcode 里打开的是同一个工程，改配置只有这一处。
pub fn configure_ios(project: &Project, device: bool) -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Err(Error::new(
            Code::Environment,
            "iOS 构建只能在 macOS 上进行",
            "iOS 工具链只有 macOS 版；别的系统上只能构建 desktop 和 android",
        ));
    }
    if toolchain::xcodebuild().is_none() {
        return Err(Error::new(
            Code::Environment,
            "找不到 xcodebuild",
            "从 App Store 安装 Xcode（Apple 的 SDK 无法由 vkx 代为分发）",
        )
        .hint("装好后执行：sudo xcode-select -s /Applications/Xcode.app"));
    }
    if !project.root.join("ios/Info.plist").is_file() {
        return Err(Error::new(
            Code::Environment,
            "工程里没有 ios/Info.plist",
            "用新版 vkx new 生成的工程才带 iOS 支持",
        ));
    }

    let cmake = toolchain::require_cmake()?;
    let slangc = toolchain::require_slangc()?;
    let moltenvk = toolchain::moltenvk_lib(device)?;

    let sysroot = if device {
        "iphoneos"
    } else {
        "iphonesimulator"
    };
    let build_dir = ios_build_dir(project, device);

    crate::generate::cmake(project)?;
    ui::step(&format!("生成 Xcode 工程 ({sysroot})"));
    let mut configure = Command::new(&cmake);
    configure
        .arg("-S")
        .arg(project.cmake_dir())
        .arg("-B")
        .arg(&build_dir)
        .arg("-G")
        .arg("Xcode")
        .arg("-DCMAKE_SYSTEM_NAME=iOS")
        .arg(format!("-DCMAKE_OSX_SYSROOT={sysroot}"))
        .arg("-DCMAKE_OSX_ARCHITECTURES=arm64")
        // MoltenVK 的二进制是按 iOS 15 构建的，部署目标不能比它低。
        .arg("-DCMAKE_OSX_DEPLOYMENT_TARGET=15.0")
        .arg(format!("-DVKX_SLANGC={}", toolchain::cmake_path(&slangc)))
        .arg(format!(
            "-DVKX_MOLTENVK_LIB={}",
            toolchain::cmake_path(&moltenvk)
        ));

    if !device {
        // 模拟器不需要签名，关掉省事。
        //
        // 真机这一支什么都不设：签名是绑 Apple 账号的（证书、描述文件、团队 ID），
        // vkx 代劳意味着要跟着 Apple 的格式变，而 vkx 发出去就不再更新了。
        // 生成的 .xcodeproj 里 Xcode 的自动签名开箱可用，那才是这件事该待的地方。
        configure
            .arg("-DCMAKE_XCODE_ATTRIBUTE_CODE_SIGNING_ALLOWED=NO")
            .arg("-DCMAKE_XCODE_ATTRIBUTE_CODE_SIGNING_REQUIRED=NO");
    }

    toolchain::run(&mut configure, "CMake 配置")?;

    let xcodeproj = build_dir.join(format!("{}.xcodeproj", project.name));
    if !xcodeproj.is_dir() {
        return Err(Error::new(
            Code::CommandFailed,
            format!("配置完成，但没找到 {}", xcodeproj.display()),
            "删掉 target/ios 后重试；仍然失败请连同上面 CMake 的输出一起反馈",
        ));
    }

    // 这个工程可以直接用 Xcode 打开，调试、签名、连真机、上架都在里面做。
    ui::info(&format!("Xcode 工程：{}", xcodeproj.display()));
    Ok(xcodeproj)
}

/// 生成真机用的 Xcode 工程，然后就交出去。
///
/// vkx 到此为止：往下的签名、真机调试、上架，都是 Xcode 和 Apple 账号的事。
pub fn generate_ios_project(project: &Project) -> Result<PathBuf> {
    let xcodeproj = configure_ios(project, true)?;
    ui::step("iOS 工程已生成");
    ui::info("接下来用 Xcode 打开它：");
    ui::info(&format!("  open {}", xcodeproj.display()));
    ui::info("在 Xcode 里选好 Team 打开自动签名，就能连真机跑、也能 Archive 上架。");
    Ok(xcodeproj)
}

fn ios_build_dir(project: &Project, device: bool) -> PathBuf {
    project
        .root
        .join("build")
        .join(if device { "ios" } else { "ios-simulator" })
}

/// 编译模拟器版。
///
/// 只有模拟器：真机构建要签名，而签名在 Xcode 里做（见 configure_ios）。
/// 要真机包就 `vkx build --target ios-device` 生成工程，然后用 Xcode 打开。
pub fn build_ios(project: &Project, profile: Profile) -> Result<PathBuf> {
    configure_ios(project, false)?;

    let cmake = toolchain::require_cmake()?;
    let build_dir = ios_build_dir(project, false);
    let sysroot = "iphonesimulator";

    ui::step("编译");
    let mut build = Command::new(&cmake);
    build
        .arg("--build")
        .arg(&build_dir)
        .arg("--config")
        .arg(profile.cmake_config())
        // xcodebuild 默认把每条编译命令都打出来，刷屏且没用。
        .arg("--")
        .arg("-quiet");
    toolchain::run(&mut build, "编译")?;

    let app = build_dir
        .join(format!("{}-{sysroot}", profile.cmake_config()))
        .join(format!("{}.app", project.name));
    if !app.is_dir() {
        return Err(Error::new(
            Code::CommandFailed,
            format!("编译完成，但没找到 {}", app.display()),
            "看上面 xcodebuild 的输出找真正的失败原因",
        ));
    }
    Ok(app)
}

pub fn run_ios(project: &Project, profile: Profile) -> Result<i32> {
    let app = build_ios(project, profile)?;

    let simulator = boot_simulator()?;
    ui::step("安装到模拟器");
    toolchain::run(
        Command::new("xcrun")
            .args(["simctl", "install", &simulator])
            .arg(&app),
        "simctl install",
    )?;

    ui::step("启动");
    toolchain::run(
        Command::new("xcrun").args([
            "simctl",
            "launch",
            // 重复 vkx run 时先把上一次的进程干掉，否则 simctl 会报已在运行。
            "--terminate-running-process",
            &simulator,
            &project.package_id,
        ]),
        "simctl launch",
    )?;

    ui::info("模拟器窗口应该已经打开了。");
    ui::info(&format!(
        "看日志：xcrun simctl spawn booted log stream --predicate 'process == \"{}\"'",
        project.name
    ));
    Ok(0)
}

/// 确保有一台已启动的模拟器，返回它的 UDID（或 "booted"）。
fn boot_simulator() -> Result<String> {
    let listing = toolchain::capture(Path::new("xcrun"), &["simctl", "list", "devices", "booted"])
        .unwrap_or_default();
    if listing.contains('(') {
        return Ok("booted".to_string());
    }

    let available = toolchain::capture(
        Path::new("xcrun"),
        &["simctl", "list", "devices", "available", "iPhone"],
    )
    .unwrap_or_default();
    let udid = available
        .lines()
        .filter_map(parse_udid)
        .next()
        .ok_or_else(|| {
            Error::new(
                Code::Environment,
                "没有可用的 iOS 模拟器",
                "Xcode → Settings → Components 里安装一个 iOS Simulator 运行时",
            )
        })?;

    ui::step("启动模拟器");
    toolchain::run(
        Command::new("xcrun").args(["simctl", "boot", &udid]),
        "simctl boot",
    )?;
    // 开机要几秒，没等完就 install 会失败。
    toolchain::run(
        Command::new("xcrun").args(["simctl", "bootstatus", &udid, "-b"]),
        "等待模拟器启动",
    )?;
    let _ = Command::new("open").args(["-a", "Simulator"]).status();
    Ok(udid)
}

fn parse_udid(line: &str) -> Option<String> {
    let start = line.find('(')? + 1;
    let end = line[start..].find(')')? + start;
    let candidate = &line[start..end];
    // UDID 形如 12345678-1234-1234-1234-123456789012
    (candidate.len() == 36 && candidate.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| candidate.to_string())
}
