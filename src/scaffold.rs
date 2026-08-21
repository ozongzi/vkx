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

#[cfg(test)]
mod tests {
    use super::{TEMPLATE, default_package_id, rewrite_path, substitute};
    use include_dir::Dir;
    use std::path::Path;

    /// 模版里出现、但故意不在 `vkx new` 时替换的占位符。
    ///
    /// `{{PROJECT_VERSION}}` 只有 macos/Info.plist 用，留到 `vkx dist` 时才按
    /// vkx.toml 的 version 填（见 dist.rs）。改版本号不该要求重新生成工程，
    /// 所以它必须一路留在文件里。
    const 打包时才填的: &[(&str, &str)] = &[("{{PROJECT_VERSION}}", "macos/Info.plist")];

    /// 找出文本里所有 `{{大写标识符}}`。
    ///
    /// 只认大写和下划线是有原因的：模版是 C++ 源码，`{{0.02f, 0.05f}}` 这种
    /// 花括号初始化满地都是，按 `{{` 一刀切会把它们全当成占位符。
    fn 占位符(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let b = text.as_bytes();
        let mut i = 0;
        while let Some(off) = text[i..].find("{{") {
            let start = i + off;
            let mut j = start + 2;
            while j < b.len() && (b[j].is_ascii_uppercase() || b[j] == b'_') {
                j += 1;
            }
            if j > start + 2 && text[j..].starts_with("}}") {
                out.push(text[start..j + 2].to_string());
            }
            i = start + 2;
        }
        out
    }

    fn 遍历(dir: &'static Dir<'static>, out: &mut Vec<(String, String)>) {
        for f in dir.files() {
            if let Some(text) = f.contents_utf8() {
                out.push((
                    f.path().to_string_lossy().replace('\\', "/"),
                    text.to_string(),
                ));
            }
        }
        for d in dir.dirs() {
            遍历(d, out);
        }
    }

    fn 模版文件() -> Vec<(String, String)> {
        let mut out = Vec::new();
        遍历(&TEMPLATE, &mut out);
        assert!(!out.is_empty(), "模版一个文件都没内嵌进来");
        out
    }

    // 往模版里加了新占位符却忘了在 substitute() 里加一行，学员拿到的就是一个
    // 字面写着 {{XXX}} 的工程——编译不报错，跑起来才发现窗口标题不对。
    #[test]
    fn 模版里的占位符都被替换掉了() {
        for (path, text) in 模版文件() {
            let 替换后 = substitute(&text, "demo", "com.example.demo");
            let 残留: Vec<_> = 占位符(&替换后)
                .into_iter()
                .filter(|p| {
                    !打包时才填的
                        .iter()
                        .any(|(白名单, 允许出现在)| p == 白名单 && path == *允许出现在)
                })
                .collect();
            assert!(残留.is_empty(), "{path} 里还剩没替换的占位符：{残留:?}");
        }
    }

    // 白名单不能长毛：留在文件里的占位符，得确认它真的还在那个文件里。
    #[test]
    fn 白名单里的占位符确实还在模版里() {
        let 文件 = 模版文件();
        for (白名单, 出现在) in 打包时才填的 {
            let (_, text) = 文件
                .iter()
                .find(|(p, _)| p == 出现在)
                .unwrap_or_else(|| panic!("白名单指向的 {出现在} 不在模版里"));
            assert!(
                text.contains(白名单),
                "{出现在} 里已经没有 {白名单} 了，白名单该删掉这一条"
            );
        }
    }

    // 花括号初始化不能被当成占位符，否则上面那条测试永远是红的。
    #[test]
    fn 花括号初始化不算占位符() {
        assert_eq!(占位符("{{0.02f, 0.05f, 1.0f}}"), Vec::<String>::new());
        assert_eq!(
            占位符("std::array<int, 2> a{{1, 2}};"),
            Vec::<String>::new()
        );
        assert_eq!(占位符("name = {{PROJECT_NAME}}"), vec!["{{PROJECT_NAME}}"]);
    }

    // Android 的 java 目录必须和包名一致，否则 MainActivity 找不到。
    #[test]
    fn 安卓包路径按包名展开() {
        let 原 = Path::new("android/app/src/main/java/vkxpackage/MainActivity.java");
        assert_eq!(
            rewrite_path(原, "tech.yinli.client").to_string_lossy(),
            "android/app/src/main/java/tech/yinli/client/MainActivity.java"
        );
    }

    // 包名不许带连字符，安卓那边是硬性要求。
    #[test]
    fn 默认包名去掉连字符() {
        assert_eq!(default_package_id("moba-client"), "com.example.mobaclient");
        assert_eq!(default_package_id("client"), "com.example.client");
    }
}
