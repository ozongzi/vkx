use std::path::{Path, PathBuf};

use crate::error::{Code, Error, Result};

/// 一个 vkx 工程：由工程根目录下的 vkx.toml 标识。
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub package_id: String,
    /// 发布版本号，用于 vkx dist 的包名。
    pub version: String,
    /// vkx.toml 里 [ios] development_team，填了才能构建 iOS 真机包。
    pub development_team: Option<String>,
}

impl Project {
    /// 从当前目录往上找 vkx.toml（跟 cargo 找 Cargo.toml 一个套路）。
    pub fn discover(start: &Path) -> Result<Self> {
        let start = start.canonicalize().map_err(|e| {
            Error::new(
                Code::Io,
                format!("无法访问 {}: {e}", start.display()),
                "确认这个目录存在且有读权限",
            )
        })?;

        for directory in start.ancestors() {
            let manifest = directory.join("vkx.toml");
            if manifest.is_file() {
                return Self::load(directory, &manifest);
            }
        }

        Err(Error::new(
            Code::NotAProject,
            "当前目录不在任何 vkx 工程里（往上都没找到 vkx.toml）",
            "先 cd 进工程目录，或用 `vkx new <名字>` 新建一个",
        ))
    }

    fn load(root: &Path, manifest: &Path) -> Result<Self> {
        let text = crate::fs::read_to_string(manifest).map_err(|e| {
            Error::new(
                Code::Io,
                format!("读不了 {}: {e}", manifest.display()),
                "确认文件存在且有读权限",
            )
        })?;

        let name = value_of(&text, "project", "name").ok_or_else(|| {
            Error::new(
                Code::BadManifest,
                format!("{} 里缺少 [project] name 字段", manifest.display()),
                "格式: name = \"mygame\"",
            )
        })?;
        let package_id = value_of(&text, "project", "package_id")
            .unwrap_or_else(|| format!("com.example.{name}"));
        let version = value_of(&text, "project", "version").unwrap_or_else(|| "0.1.0".to_string());
        let development_team = value_of(&text, "ios", "development_team");

        Ok(Self {
            root: root.to_path_buf(),
            name,
            package_id,
            version,
            development_team,
        })
    }

    pub fn build_dir(&self, profile: &str) -> PathBuf {
        self.root.join("build").join(profile)
    }
}

/// 在指定的 [段] 里取 `key = "value"`。
///
/// vkx.toml 是我们自己生成的，只有这一种写法，不值得为它引一个 TOML 库。
/// 但段落必须认——[project] 和 [vkx] 下都有 version 这个键。
fn value_of(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = name.trim().to_string();
            continue;
        }
        if current != section {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let value = right.trim().trim_matches('"').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Android 的包名会原样变成 Java 的 package 语句，撞上关键字就编译不过。
const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "true",
    "false",
    "null",
    "record",
    "sealed",
    "permits",
    "var",
    "yield",
];

pub fn validate_package_id(package_id: &str) -> Result<()> {
    for segment in package_id.split('.') {
        if segment.is_empty() {
            return Err(Error::new(
                Code::NotAProject,
                format!("包名 `{package_id}` 里有空的一段"),
                "形如 com.example.mygame",
            ));
        }
        if !segment.chars().next().unwrap().is_ascii_alphabetic() {
            return Err(Error::new(
                Code::BadManifest,
                format!("包名的每一段都要以字母开头：`{segment}`"),
                "例如 com.example.game；数字和下划线可以出现在段中间但不能开头",
            ));
        }
        if JAVA_KEYWORDS.contains(&segment) {
            return Err(Error::new(
                Code::BadManifest,
                format!("包名里的 `{segment}` 是 Java 关键字，Android 编不过"),
                "换一个不含 Java 关键字的包名，例如 com.example.game",
            ));
        }
    }
    Ok(())
}

/// 工程名要同时能当 CMake target、可执行文件名和 Android 包名的一段。
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            Code::BadManifest,
            "工程名不能为空",
            "在 vkx.toml 的 [project] 里填 name，或者 vkx new 时用命令行给出",
        ));
    }
    if !name.chars().next().unwrap().is_ascii_alphabetic() {
        return Err(Error::new(
            Code::BadManifest,
            format!("工程名 `{name}` 必须以英文字母开头"),
            "它会被用作可执行文件名和 Android 包名的一部分",
        ));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && *c != '_' && *c != '-')
    {
        return Err(Error::new(
            Code::NotAProject,
            format!("工程名 `{name}` 里有不允许的字符 `{bad}`"),
            "只能用字母、数字、下划线和连字符",
        ));
    }
    Ok(())
}
