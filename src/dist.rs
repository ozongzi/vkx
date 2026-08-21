//! `vkx dist`：把工程打成可以直接发给别人的包。
//!
//! 和 `vkx build` 的区别是产物要能离开这台机器：
//!   macOS   .app 包（内嵌 MoltenVK，ad-hoc 签名）+ .dmg
//!   Windows .zip（可执行文件是静态链接的，解压即用）
//!   Linux   .tar.gz
//!   Android 签名 APK + AAB（AAB 是上架 Google Play 用的格式）
//!
//! 全部产出在工程的 dist/ 目录下。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::builder::{self, Profile};
use crate::error::{Code, Error, Result};
use crate::mobile;
use crate::project::Project;
use crate::toolchain;
use crate::ui;

pub fn dist_dir(project: &Project) -> PathBuf {
    project.root.join("dist")
}

fn prepare_dist_dir(project: &Project) -> Result<PathBuf> {
    let dir = dist_dir(project);
    crate::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 包名统一成 <工程名>-<版本>-<平台>。
fn package_name(project: &Project, platform: &str) -> String {
    format!("{}-{}-{platform}", project.name, project.version)
}

// ===========================================================================
// 桌面
// ===========================================================================

pub fn dist_desktop(project: &Project) -> Result<PathBuf> {
    // 分发一律用 Release。
    let executable = builder::build(project, Profile::Release)?;
    let dist = prepare_dist_dir(project)?;

    if cfg!(target_os = "macos") {
        dist_macos(project, &executable, &dist)
    } else if cfg!(windows) {
        dist_windows(project, &executable, &dist)
    } else {
        dist_linux(project, &executable, &dist)
    }
}

/// 组装 .app 包，再压成 .dmg。
///
/// MoltenVK 要一起放进包里：用户的机器上不一定装了 Vulkan，
/// main.cpp 会在系统里找不到时回头加载 Contents/Frameworks 里的这一份。
fn dist_macos(project: &Project, executable: &Path, dist: &Path) -> Result<PathBuf> {
    let app = dist.join(format!("{}.app", project.name));
    let contents = app.join("Contents");
    let macos_dir = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");

    ui::step("组装 .app");
    if app.exists() {
        crate::fs::remove_dir_all(&app)?;
    }
    crate::fs::create_dir_all(&macos_dir)?;
    crate::fs::create_dir_all(&frameworks)?;

    crate::fs::copy(executable, &macos_dir.join(&project.name))?;
    crate::fs::write(&contents.join("PkgInfo"), "APPL????")?;

    let plist_template = project.root.join("macos/Info.plist");
    if !plist_template.is_file() {
        return Err(Error::new(
            Code::CommandFailed,
            format!("缺少 {}", plist_template.display()),
            "用新版 vkx new 生成的工程才带 macOS 打包配置",
        ));
    }
    let plist = crate::fs::read_to_string(&plist_template)?
        .replace("{{PROJECT_VERSION}}", &project.version);
    crate::fs::write(&contents.join("Info.plist"), plist)?;

    let moltenvk = toolchain::moltenvk_dylib_dir()
        .map(|dir| dir.join("libMoltenVK.dylib"))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            Error::new(
                Code::CommandFailed,
                "找不到 libMoltenVK.dylib，没法把 Vulkan 打进包里",
                "重新运行安装脚本，它会装 MoltenVK",
            )
        })?;
    crate::fs::copy(&moltenvk, &frameworks.join("libMoltenVK.dylib"))?;

    // ad-hoc 签名：不需要开发者账号，但没有它 macOS 会直接拒绝运行。
    // 要给别人下载还得做公证（notarization），那需要付费账号。
    ui::step("签名 (ad-hoc)");
    toolchain::run(
        Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&app),
        "codesign",
    )?;

    ui::step("打包 .dmg");
    let dmg = dist.join(format!("{}.dmg", package_name(project, "macos")));
    let _ = crate::fs::remove_file(&dmg);
    toolchain::run(
        Command::new("hdiutil")
            .args(["create", "-quiet", "-volname", &project.name, "-srcfolder"])
            .arg(&app)
            .args(["-ov", "-format", "UDZO"])
            .arg(&dmg),
        "hdiutil",
    )?;

    Ok(dmg)
}

fn dist_windows(project: &Project, executable: &Path, dist: &Path) -> Result<PathBuf> {
    // Windows 上可执行文件是静态链接的，Vulkan 由显卡驱动提供，包里只需要它自己。
    let archive = dist.join(format!("{}.zip", package_name(project, "windows")));
    let _ = crate::fs::remove_file(&archive);

    ui::step("打包 .zip");
    let staging = dist.join(".staging");
    stage_single_file(&staging, executable, &format!("{}.exe", project.name))?;
    toolchain::run(
        Command::new("tar")
            .arg("-a")
            .arg("-c")
            .arg("-f")
            .arg(&archive)
            .arg("-C")
            .arg(&staging)
            .arg("."),
        "打包",
    )?;
    crate::fs::remove_dir_all(&staging)?;
    Ok(archive)
}

fn dist_linux(project: &Project, executable: &Path, dist: &Path) -> Result<PathBuf> {
    let archive = dist.join(format!("{}.tar.gz", package_name(project, "linux")));
    let _ = crate::fs::remove_file(&archive);

    ui::step("打包 .tar.gz");
    let staging = dist.join(".staging");
    stage_single_file(&staging, executable, &project.name)?;
    toolchain::run(
        Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&staging)
            .arg("."),
        "打包",
    )?;
    crate::fs::remove_dir_all(&staging)?;
    Ok(archive)
}

fn stage_single_file(staging: &Path, source: &Path, name: &str) -> Result<()> {
    if staging.exists() {
        crate::fs::remove_dir_all(staging)?;
    }
    crate::fs::create_dir_all(staging)?;
    crate::fs::copy(source, &staging.join(name))?;
    Ok(())
}

// ===========================================================================
// Android
// ===========================================================================

/// 出签名 APK 和 AAB。APK 用来直接安装，AAB 用来上架 Google Play。
pub fn dist_android(project: &Project) -> Result<Vec<PathBuf>> {
    let apk = mobile::build_android(project, Profile::Release)?;
    if apk.to_string_lossy().contains("-unsigned") {
        return Err(Error::new(
            Code::CommandFailed,
            "产出的 APK 没有签名，不能分发",
            "检查 android/keystore.properties 是否存在（vkx new 时会生成）",
        ));
    }

    let aab = mobile::bundle_android(project)?;
    let dist = prepare_dist_dir(project)?;

    let mut outputs = Vec::new();
    for (source, extension) in [(apk, "apk"), (aab, "aab")] {
        let target = dist.join(format!("{}.{extension}", package_name(project, "android")));
        crate::fs::copy(&source, &target)?;
        outputs.push(target);
    }
    Ok(outputs)
}

// ===========================================================================
// iOS
// ===========================================================================
