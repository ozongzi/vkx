//! `vkx add` / `vkx remove` / `vkx deps`
//!
//! 依赖不追版本：vkx 的版本号就是依赖集的版本号，一个 vkx 对应一套确定的库，
//! 所以没有版本求解、没有锁文件。
//!
//! 只有「要从源码编」的库需要开关，因为它们各自是几分钟的编译时间。预编译的
//! C 库和只有头文件的库永远可用——链接一个没被引用的静态库几乎不花钱。

use crate::error::{Code, Error, Result};
use crate::project::{Project, SOURCE_LIBS};
use crate::ui;

/// SDK 包里那些不用声明就能用的东西，`vkx deps` 会列出来。
const ALWAYS_AVAILABLE: &[(&str, &str, &str)] = &[
    ("SDL3", "预编译", "窗口、输入、音频、文件对话框"),
    (
        "Vulkan-Headers + volk",
        "预编译",
        "Vulkan 头文件和函数指针加载",
    ),
    ("mbedTLS", "预编译", "TLS，给 cpp-httplib 当后端"),
    ("zlib", "预编译", "压缩"),
    ("FreeType", "预编译", "字体栅格化"),
    ("cpp-httplib", "头文件", "HTTP 客户端和服务端"),
    ("stb_image", "头文件", "PNG / JPEG 解码"),
    ("GLM", "头文件", "向量和矩阵"),
];

fn find(name: &str) -> Result<&'static str> {
    SOURCE_LIBS
        .iter()
        .find(|lib| lib.key == name)
        .map(|lib| lib.key)
        .ok_or_else(|| {
            let known: Vec<&str> = SOURCE_LIBS.iter().map(|l| l.key).collect();
            Error::new(
                Code::Usage,
                format!("没有叫 `{name}` 的库"),
                format!("可选：{}", known.join("、")),
            )
            .hint("预编译库和头文件库不用 add，直接 #include 就能用；`vkx deps` 有完整清单")
        })
}

/// 改 vkx.toml 里 [libs] 下某一项的开关。
fn set(project: &Project, name: &str, on: bool) -> Result<()> {
    let manifest = project.root.join("vkx.toml");
    let text = crate::fs::read_to_string(&manifest)?;

    let mut out = Vec::new();
    let mut section = String::new();
    let mut written = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = inner.trim().to_string();
        }
        if section == "libs"
            && let Some((left, right)) = trimmed.split_once('=')
            && left.trim() == name
        {
            // 保留行尾注释
            let comment = right
                .find(" #")
                .or_else(|| right.find("\t#"))
                .map(|at| right[at..].to_string())
                .unwrap_or_default();
            out.push(format!("{name} = {on}{comment}"));
            written = true;
            continue;
        }
        out.push(line.to_string());
    }

    if !written {
        // 没有 [libs] 段就补一个
        if !text.contains("[libs]") {
            out.push(String::new());
            out.push("[libs]".to_string());
        }
        out.push(format!("{name} = {on}"));
    }

    crate::fs::write(&manifest, out.join("\n") + "\n")
}

pub fn add(project: &Project, name: &str) -> Result<u8> {
    let key = find(name)?;
    if project.libs.iter().any(|l| l == key) {
        ui::info(&format!("{key} 已经打开了。"));
        return Ok(0);
    }
    set(project, key, true)?;
    ui::step(&format!("已打开 {key}"));
    ui::info("下次 vkx build 会把它编进来，第一次会多花几分钟。");
    Ok(0)
}

pub fn remove(project: &Project, name: &str) -> Result<u8> {
    let key = find(name)?;
    if !project.libs.iter().any(|l| l == key) {
        ui::info(&format!("{key} 本来就没打开。"));
        return Ok(0);
    }
    set(project, key, false)?;
    ui::step(&format!("已关闭 {key}"));
    Ok(0)
}

pub fn list(project: &Project) -> Result<u8> {
    ui::step("随时可用（不用声明，直接 #include）");
    for (name, kind, about) in ALWAYS_AVAILABLE {
        ui::info(&format!("{name:<24} {kind:<6} {about}"));
    }

    ui::step("要从源码编（vkx add 打开）");
    for lib in SOURCE_LIBS {
        let on = project.libs.iter().any(|l| l == lib.key);
        ui::info(&format!(
            "{:<24} {:<6} {}",
            lib.key,
            if on { "已打开" } else { "关闭" },
            lib.about
        ));
    }
    Ok(0)
}
