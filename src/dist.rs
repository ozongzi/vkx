//! `vkx dist`：把工程打成可以直接发给别人的包。
//!
//! 和 `vkx build` 的区别是产物要能离开这台机器：
//!   macOS   .app 包（内嵌 MoltenVK，ad-hoc 签名）+ .dmg
//!   Windows .zip（可执行文件是静态链接的，解压即用）
//!   Linux   .tar.gz
//!   Android 签名 APK + AAB（AAB 是上架 Google Play 用的格式）
//!   iOS     .ipa（需要开发者证书）
//!
//! 全部产出在工程的 dist/ 目录下。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::builder::{self, Profile};
use crate::error::{Error, Result};
use crate::mobile;
use crate::project::Project;
use crate::toolchain;
use crate::ui;

pub fn dist_dir(project: &Project) -> PathBuf {
    project.root.join("dist")
}

fn prepare_dist_dir(project: &Project) -> Result<PathBuf> {
    let dir = dist_dir(project);
    std::fs::create_dir_all(&dir)?;
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
        std::fs::remove_dir_all(&app)?;
    }
    std::fs::create_dir_all(&macos_dir)?;
    std::fs::create_dir_all(&frameworks)?;

    std::fs::copy(executable, macos_dir.join(&project.name))?;
    std::fs::write(contents.join("PkgInfo"), "APPL????")?;

    let plist_template = project.root.join("macos/Info.plist");
    if !plist_template.is_file() {
        return Err(Error::new(format!("缺少 {}", plist_template.display()))
            .hint("用新版 vkx new 生成的工程才带 macOS 打包配置"));
    }
    let plist =
        std::fs::read_to_string(&plist_template)?.replace("{{PROJECT_VERSION}}", &project.version);
    std::fs::write(contents.join("Info.plist"), plist)?;

    let moltenvk = toolchain::moltenvk_dylib_dir()
        .map(|dir| dir.join("libMoltenVK.dylib"))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            Error::new("找不到 libMoltenVK.dylib，没法把 Vulkan 打进包里")
                .hint("重新运行安装脚本，它会装 MoltenVK")
        })?;
    std::fs::copy(&moltenvk, frameworks.join("libMoltenVK.dylib"))?;

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
    let _ = std::fs::remove_file(&dmg);
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
    let _ = std::fs::remove_file(&archive);

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
    std::fs::remove_dir_all(&staging)?;
    Ok(archive)
}

fn dist_linux(project: &Project, executable: &Path, dist: &Path) -> Result<PathBuf> {
    let archive = dist.join(format!("{}.tar.gz", package_name(project, "linux")));
    let _ = std::fs::remove_file(&archive);

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
    std::fs::remove_dir_all(&staging)?;
    Ok(archive)
}

fn stage_single_file(staging: &Path, source: &Path, name: &str) -> Result<()> {
    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    std::fs::create_dir_all(staging)?;
    std::fs::copy(source, staging.join(name))?;
    Ok(())
}

// ===========================================================================
// Android
// ===========================================================================

/// 出签名 APK 和 AAB。APK 用来直接安装，AAB 用来上架 Google Play。
pub fn dist_android(project: &Project) -> Result<Vec<PathBuf>> {
    let apk = mobile::build_android(project, Profile::Release)?;
    if apk.to_string_lossy().contains("-unsigned") {
        return Err(Error::new("产出的 APK 没有签名，不能分发")
            .hint("检查 android/keystore.properties 是否存在（vkx new 时会生成）"));
    }

    let aab = mobile::bundle_android(project)?;
    let dist = prepare_dist_dir(project)?;

    let mut outputs = Vec::new();
    for (source, extension) in [(apk, "apk"), (aab, "aab")] {
        let target = dist.join(format!("{}.{extension}", package_name(project, "android")));
        std::fs::copy(&source, &target)?;
        outputs.push(target);
    }
    Ok(outputs)
}

// ===========================================================================
// iOS
// ===========================================================================

/// 出 .ipa。需要 vkx.toml 里填了开发者团队 ID，否则签不了名。
pub fn dist_ios(project: &Project) -> Result<PathBuf> {
    let team = project.development_team.clone().ok_or_else(|| {
        Error::new("iOS 分发包必须签名，但没有配置开发者团队")
            .hint("在 vkx.toml 里填：[ios] development_team = \"你的团队 ID\"")
    })?;

    // 先让 CMake 生成好 Xcode 工程（真机配置）。
    let xcodeproj = mobile::configure_ios(project, true)?;
    let build_dir = xcodeproj.parent().unwrap_or(&project.root).to_path_buf();
    let archive = build_dir.join(format!("{}.xcarchive", project.name));

    ui::step("xcodebuild archive");
    toolchain::run(
        Command::new("xcodebuild")
            .arg("-project")
            .arg(&xcodeproj)
            .args([
                "-scheme",
                &project.name,
                "-configuration",
                "Release",
                "-destination",
                "generic/platform=iOS",
                "-archivePath",
            ])
            .arg(&archive)
            .args(["archive", "-quiet"]),
        "xcodebuild archive",
    )?;

    // 导出用的选项文件，method=development 表示导出给自己的设备装。
    let options = build_dir.join("ExportOptions.plist");
    std::fs::write(
        &options,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n<dict>\n\
             \t<key>method</key>\n\t<string>development</string>\n\
             \t<key>teamID</key>\n\t<string>{team}</string>\n\
             \t<key>signingStyle</key>\n\t<string>automatic</string>\n\
             \t<key>stripSwiftSymbols</key>\n\t<true/>\n\
             </dict>\n</plist>\n"
        ),
    )?;

    let dist = prepare_dist_dir(project)?;
    let export_dir = dist.join(".ios-export");
    if export_dir.exists() {
        std::fs::remove_dir_all(&export_dir)?;
    }

    ui::step("导出 .ipa");
    toolchain::run(
        Command::new("xcodebuild")
            .arg("-exportArchive")
            .arg("-archivePath")
            .arg(&archive)
            .arg("-exportOptionsPlist")
            .arg(&options)
            .arg("-exportPath")
            .arg(&export_dir)
            .arg("-quiet"),
        "xcodebuild exportArchive",
    )?;

    // 导出目录里就一个 .ipa，挪到 dist/ 下并按统一规则命名。
    let exported = std::fs::read_dir(&export_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "ipa"))
        .ok_or_else(|| Error::new("导出完成，但没找到 .ipa"))?;

    let target = dist.join(format!("{}.ipa", package_name(project, "ios")));
    std::fs::copy(&exported, &target)?;
    std::fs::remove_dir_all(&export_dir)?;
    Ok(target)
}
