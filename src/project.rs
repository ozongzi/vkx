use std::path::{Path, PathBuf};

use crate::error::{Code, Error, Result};

/// 一个 vkx 工程：由工程根目录下的 vkx.toml 标识。
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub package_id: String,
    /// 发布版本号，用于 vkx dist 的包名。
    pub version: String,
    /// [project] dependencies 里声明的依赖，按表里的顺序规范化过。
    /// 决定链哪些库、暴露哪些头文件。
    pub dependencies: Vec<String>,
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
        // Windows 上 canonicalize() 返回的是 `\\?\D:\x` 这种扩展长度路径。工程根
        // 目录会一路传进 CMake 的 -S/-B，而 CMake 不认这个前缀，会把它当成网络
        // 路径。这里就地摘掉，后面所有派生路径都干净。
        let start = crate::toolchain::plain_path(&start);

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
        let dependencies = array_of(&text, "project", "dependencies");
        // 名字写错了要当场说，别等到 CMake 那边报一句 find_package 失败。
        for name in &dependencies {
            if find_dependency(name).is_none() {
                return Err(Error::new(
                    Code::BadManifest,
                    format!(
                        "{} 里的 dependencies 有个不认识的名字：{name}",
                        manifest.display()
                    ),
                    "`vkx deps` 列出全部可用的名字",
                ));
            }
        }
        // 展开传递依赖，再按表的顺序排。
        //
        // 顺序是硬要求：被依赖的必须先 find_package，否则后面那个包的
        // link interface 里会出现一个「找不到的 target」。表本身就是按
        // 依赖顺序写的，所以过一遍表即可。
        let dependencies = expand(&dependencies);

        Ok(Self {
            root: root.to_path_buf(),
            name,
            package_id,
            version,
            dependencies,
        })
    }

    /// 生成的 CMakeLists 所在目录，也是 cmake 的 source dir。
    pub fn cmake_dir(&self) -> PathBuf {
        self.root.join("target")
    }

    pub fn build_dir(&self, profile: &str) -> PathBuf {
        self.root.join("target").join(profile)
    }
}

/// 在指定的 [段] 里取 `key = "value"`。
///
/// 读一个字符串数组，比如 dependencies = ["SDL3", "FreeType"]。
///
/// 支持写成一行，也支持跨行——`vkx add` 加到第四个之后就会折行，
/// 而且读者自己也会手写。数组里的注释和尾逗号都容忍。
fn array_of(text: &str, section: &str, key: &str) -> Vec<String> {
    let mut current = String::new();
    let mut collecting = false;
    let mut raw = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !collecting {
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some(name) = trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                current = name.trim().to_string();
                continue;
            }
            if current != section {
                continue;
            }
            let Some((left, right)) = trimmed.split_once('=') else {
                continue;
            };
            if left.trim() != key {
                continue;
            }
            let right = right.trim_start();
            if !right.starts_with('[') {
                return Vec::new();
            }
            collecting = true;
            raw.push_str(&right[1..]);
        } else {
            raw.push(' ');
            raw.push_str(trimmed);
        }
        if let Some(end) = raw.find(']') {
            raw.truncate(end);
            break;
        }
    }
    if !collecting {
        return Vec::new();
    }
    raw.split(',')
        .map(|item| item.split('#').next().unwrap_or("").trim())
        .filter_map(|item| {
            let item = item.trim_matches(|c| c == '"' || c == '\'');
            (!item.is_empty()).then(|| item.to_string())
        })
        .collect()
}

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
        // 去掉行尾注释：只在 # 前面有空白时才算注释，值里的 # 不受影响。
        let right = match right.find(" #").or_else(|| right.find("\t#")) {
            Some(at) => &right[..at],
            None => right,
        };
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

/// 要从源码编译的库。预编译的 C 库和只有头文件的库永远可用，不需要开关——
/// 链接一个没被引用的静态库几乎不花钱，而这两个各自是几分钟的编译时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 一个可选依赖：vkx.toml 的 dependencies 里写它的名字，就链进来。
///
/// 全部是预编译好的（见离线包里的 libs 组件），所以开关只影响链接和头文件，
/// 不影响构建时间——这和以前那套「从源码编，打开一个多等几分钟」不一样。
pub struct Dependency {
    /// vkx.toml 里写的名字，大小写不敏感。
    pub name: &'static str,
    /// CMake 里 find_package 的包名。空表示不需要 find_package
    /// （纯头文件的几个直接躺在 include/ 下）。
    pub package: &'static str,
    /// 要链的 target。空表示只有头文件。
    pub targets: &'static [&'static str],
    /// 这个库自己要用到的别的库。
    ///
    /// 各家的 CMake 配置包会把这些写进 link interface，但**不**替你
    /// find_package，于是报「target ZLIB::ZLIB not found」——那个错离真正的
    /// 原因（zlib 没声明）隔着一层。所以在这里记下来，由 vkx 自动补齐：
    /// 学员不该需要知道 FreeType 用了 zlib。
    pub requires: &'static [&'static str],
    /// 非空时，生成的 find_package 会包在 if(<这个条件>) 里。
    pub guard: &'static str,
    /// 走 CONFIG 模式（用库自带的 xxxConfig.cmake）还是 CMake 自带的
    /// Findxxx.cmake 模块。zlib 不发配置包，只能走模块；写成 CONFIG 会报
    /// 「找不到 ZLIBConfig.cmake」。
    pub config_mode: bool,
    pub about: &'static str,
}

/// 可选依赖表。名字就是 vkx.toml 里 dependencies 数组里写的东西。
///
/// 顺序有讲究：被依赖的排在前面（FreeType 要 zlib，GNS 要 protobuf 和 OpenSSL），
/// 生成 CMakeLists 时按这个顺序 find_package，后面的才找得到前面的。
pub const DEPENDENCIES: &[Dependency] = &[
    Dependency {
        name: "SDL3",
        package: "SDL3",
        targets: &["SDL3::SDL3"],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "窗口、输入、音频、文件对话框",
    },
    Dependency {
        name: "Vulkan",
        package: "", // volk 由生成的 CMakeLists 自己编，见 generate.rs
        targets: &[],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "Vulkan 头文件和 volk 函数指针加载",
    },
    Dependency {
        name: "zlib",
        package: "ZLIB",
        targets: &["ZLIB::ZLIB"],
        requires: &[],
        guard: "",
        config_mode: false,
        about: "压缩",
    },
    Dependency {
        name: "FreeType",
        package: "Freetype",
        targets: &["Freetype::Freetype"],
        requires: &["zlib"],
        guard: "",
        config_mode: true,
        about: "字体栅格化",
    },
    Dependency {
        name: "mbedTLS",
        package: "MbedTLS",
        targets: &[
            "MbedTLS::mbedtls",
            "MbedTLS::mbedx509",
            "MbedTLS::mbedcrypto",
        ],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "TLS，给 cpp-httplib 当后端",
    },
    Dependency {
        name: "OpenSSL",
        package: "OpenSSL",
        targets: &["OpenSSL::SSL", "OpenSSL::Crypto"],
        requires: &[],
        guard: "NOT WIN32",
        config_mode: true,
        about: "加密。GameNetworkingSockets 在非 Windows 上要它",
    },
    Dependency {
        name: "protobuf",
        package: "protobuf",
        targets: &["protobuf::libprotobuf"],
        requires: &["zlib"],
        guard: "",
        config_mode: true,
        about: "序列化。GameNetworkingSockets 要它",
    },
    Dependency {
        name: "GameNetworkingSockets",
        package: "GameNetworkingSockets",
        targets: &["GameNetworkingSockets::static"],
        requires: &["protobuf", "OpenSSL"],
        guard: "",
        config_mode: true,
        about: "局内实时传输（UDP、加密、P2P 打洞）",
    },
    Dependency {
        name: "Jolt",
        package: "Jolt",
        targets: &["Jolt::Jolt"],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "物理引擎",
    },
    Dependency {
        name: "GLM",
        package: "",
        targets: &[],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "向量和矩阵（纯头文件）",
    },
    Dependency {
        name: "cpp-httplib",
        package: "",
        targets: &[],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "HTTP 客户端和服务端（纯头文件）",
    },
    Dependency {
        name: "stb",
        package: "",
        targets: &[],
        requires: &[],
        guard: "",
        config_mode: true,
        about: "PNG / JPEG 编解码（纯头文件）",
    },
];

/// 把声明的依赖展开成完整闭包，并按表的顺序排好。
///
/// 各家的 CMake 配置包把自己用到的库写进 link interface 却不 find_package，
/// 所以少声明一个 zlib，报出来的是「target ZLIB::ZLIB not found」——离真正的
/// 原因隔着一层。这里替学员补齐。
fn expand(declared: &[String]) -> Vec<String> {
    let mut want: Vec<&'static str> = Vec::new();
    let mut queue: Vec<&'static str> = declared
        .iter()
        .filter_map(|n| find_dependency(n).map(|d| d.name))
        .collect();
    while let Some(name) = queue.pop() {
        if want.contains(&name) {
            continue;
        }
        want.push(name);
        if let Some(dep) = find_dependency(name) {
            queue.extend(dep.requires.iter().copied());
        }
    }
    DEPENDENCIES
        .iter()
        .filter(|d| want.contains(&d.name))
        .map(|d| d.name.to_string())
        .collect()
}

/// 按名字找一个依赖，大小写不敏感。
pub fn find_dependency(name: &str) -> Option<&'static Dependency> {
    DEPENDENCIES
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
}
