//! Android 的 release 签名。
//!
//! `vkx new` 会直接生成一份 keystore，工程开箱就能出签名包。密码是随机的，
//! 连同 keystore 一起放在 android/ 下，并且都在 .gitignore 里——它只是给
//! 你自己调试和内部分发用的，上架商店请换成你自己保管的正式密钥。

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::Result;
use crate::toolchain;
use crate::ui;

pub const KEYSTORE_RELATIVE: &str = "keystore/release.jks";
pub const PROPERTIES_FILE: &str = "keystore.properties";
const ALIAS: &str = "release";

/// 工程里是否已经有可用的签名配置。
pub fn is_configured(project_root: &Path) -> bool {
    let android = project_root.join("android");
    android.join(PROPERTIES_FILE).is_file() && android.join(KEYSTORE_RELATIVE).is_file()
}

/// 生成 keystore 和 keystore.properties。已存在则直接返回。
pub fn ensure_keystore(project_root: &Path, package_id: &str) -> Result<PathBuf> {
    let android = project_root.join("android");
    let keystore = android.join(KEYSTORE_RELATIVE);
    let properties = android.join(PROPERTIES_FILE);

    if keystore.is_file() && properties.is_file() {
        return Ok(keystore);
    }

    let keytool = toolchain::keytool()?;
    let password = random_password();

    if let Some(parent) = keystore.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 重新生成时先清掉半成品，否则 keytool 会往旧库里追加。
    let _ = std::fs::remove_file(&keystore);

    ui::step("生成 Android release 签名密钥");
    let mut command = Command::new(&keytool);
    command
        .arg("-genkeypair")
        .arg("-storetype")
        .arg("PKCS12")
        .arg("-keystore")
        .arg(&keystore)
        .arg("-alias")
        .arg(ALIAS)
        .arg("-keyalg")
        .arg("RSA")
        .arg("-keysize")
        .arg("2048")
        .arg("-validity")
        .arg("10000") // 约 27 年，够长；商店要求有效期到 2033 年之后
        .arg("-storepass")
        .arg(&password)
        .arg("-keypass")
        .arg(&password)
        .arg("-dname")
        .arg(format!("CN={package_id}, OU=vkx, O=vkx, C=CN"))
        .stdout(std::process::Stdio::null());
    toolchain::run(&mut command, "keytool 生成密钥")?;

    let content = format!(
        "# 由 vkx 生成。这份文件和 keystore 都不要提交进版本库。\n\
         # 上架应用商店请换成你自己保管的正式密钥。\n\
         storeFile={KEYSTORE_RELATIVE}\n\
         storePassword={password}\n\
         keyAlias={ALIAS}\n\
         keyPassword={password}\n"
    );
    std::fs::write(&properties, content)?;

    ui::info(&format!("密钥 {}", keystore.display()));
    ui::info(&format!("口令 {}", properties.display()));
    Ok(keystore)
}

/// `vkx new` 时调用：生成失败不影响建工程，提示一声即可。
pub fn setup_for_new_project(project_root: &Path, package_id: &str) {
    if let Err(error) = ensure_keystore(project_root, package_id) {
        ui::warn(&format!("没能生成 Android 签名密钥：{}", error.message));
        ui::info("不影响桌面和 iOS；下次执行 release 构建时会再试一次。");
    }
}

/// 用操作系统的随机源造一个 32 位十六进制口令。
/// RandomState 的种子来自系统随机数，够这个用途了。
fn random_password() -> String {
    let mut password = String::with_capacity(32);
    for salt in 0..2u64 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.write_u64(std::process::id() as u64);
        password.push_str(&format!("{:016x}", hasher.finish()));
    }
    password
}
