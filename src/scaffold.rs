use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};

use crate::error::{Code, Context, Error, Result};
use crate::project;
use crate::ui;

/// 模版整个内嵌进二进制：`vkx new` 不联网也能用，
/// 而且模版版本和 vkx 版本永远是配套的。
static TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/template");

pub struct NewOptions {
    pub name: String,
    pub path: Option<PathBuf>,
    pub package_id: String,
}

/// 工程要生成到哪个目录：没给 --path 就是当前目录下的同名目录。
pub fn target_root(name: &str, path: Option<&Path>) -> Result<PathBuf> {
    match path {
        Some(path) => Ok(path.to_path_buf()),
        None => Ok(std::env::current_dir()
            .context(Code::Io, "取当前目录", "确认当前目录还存在且有读权限")?
            .join(name)),
    }
}

/// 目标目录必须不存在或为空，否则拒绝覆盖。
pub fn ensure_available(root: &Path) -> Result<()> {
    if root.exists() && crate::fs::read_dir(root)?.into_iter().next().is_some() {
        return Err(Error::new(
            Code::Io,
            format!("{} 已存在且非空", root.display()),
            "换个名字，或先把那个目录清掉",
        ));
    }
    Ok(())
}

/// 由工程名推出默认包名。Android 的包名不允许连字符，先去掉。
pub fn default_package_id(name: &str) -> String {
    format!("com.example.{}", name.replace('-', ""))
}

pub fn create(options: &NewOptions) -> Result<PathBuf> {
    project::validate_name(&options.name)?;
    project::validate_package_id(&options.package_id)?;

    let root = target_root(&options.name, options.path.as_deref())?;
    ensure_available(&root)?;

    ui::step(&format!("创建工程 {}", root.display()));
    crate::fs::create_dir_all(&root)?;
    write_dir(&TEMPLATE, &root, &options.name, &options.package_id)?;

    Ok(root)
}

fn write_dir(dir: &Dir<'_>, root: &Path, name: &str, package_id: &str) -> Result<()> {
    for entry in dir.dirs() {
        let target = root.join(rewrite_path(entry.path(), package_id));
        crate::fs::create_dir_all(&target)?;
        write_dir(entry, root, name, package_id)?;
    }

    for file in dir.files() {
        let target = root.join(rewrite_path(file.path(), package_id));
        if let Some(parent) = target.parent() {
            crate::fs::create_dir_all(parent)?;
        }

        match std::str::from_utf8(file.contents()) {
            Ok(text) => crate::fs::write(&target, substitute(text, name, package_id))?,
            Err(_) => crate::fs::write(&target, file.contents())?,
        }
    }
    Ok(())
}

/// Android 的 java 源码目录必须和包名对应，模版里放在
/// android/app/src/main/java/vkxpackage/ 下，生成时展开成真实包路径。
fn rewrite_path(path: &Path, package_id: &str) -> PathBuf {
    let package_path = package_id.replace('.', "/");
    let text = path
        .to_string_lossy()
        .replace("java/vkxpackage", &format!("java/{package_path}"));
    PathBuf::from(text)
}

fn substitute(text: &str, name: &str, package_id: &str) -> String {
    text.replace("{{PROJECT_NAME}}", name)
        .replace("{{PACKAGE_ID}}", package_id)
        .replace("{{VKX_VERSION}}", env!("CARGO_PKG_VERSION"))
}
