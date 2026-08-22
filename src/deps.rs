//! `vkx add` / `vkx remove` / `vkx deps`
//!
//! 依赖不追版本：vkx 的版本号就是依赖集的版本号，一个 vkx 对应一套确定的库，
//! 所以没有版本求解、没有锁文件。
//!
//! 全部库都是预编译好的（离线包里的 libs 组件，一个 target 一份），所以这几个
//! 命令只改 vkx.toml 里的 dependencies 数组——它决定链哪些库、暴露哪些头文件，
//! 不影响构建时间。以前那套「打开一个就多等几分钟编译」的开关已经没有了。

use crate::error::{Code, Error, Result};
use crate::project::{DEPENDENCIES, Project, find_dependency};
use crate::ui;

/// 把名字规范成表里的写法。不认识就报错，并把可用的名字列出来——
/// 让人猜大小写或者去翻文档是不必要的。
fn canonical(name: &str) -> Result<&'static str> {
    find_dependency(name).map(|d| d.name).ok_or_else(|| {
        Error::new(
            Code::Usage,
            format!("没有叫 {name} 的依赖"),
            format!(
                "可用的：{}",
                DEPENDENCIES
                    .iter()
                    .map(|d| d.name)
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        )
    })
}

/// 改写 vkx.toml 里 [project] 段的 dependencies 数组。
///
/// 手写的 TOML 改写，理由和 project.rs 里的解析器一样：这个文件是我们自己
/// 生成的，只有一种写法。但有一点要当心——不能把读者写的注释和字段顺序打乱，
/// 所以这里是就地替换那一段，而不是整个文件重新序列化。
fn write_dependencies(project: &Project, deps: &[&str]) -> Result<()> {
    let manifest = project.root.join("vkx.toml");
    let text = crate::fs::read_to_string(&manifest)?;

    let rendered = if deps.is_empty() {
        "dependencies = []".to_string()
    } else if deps.iter().map(|d| d.len() + 4).sum::<usize>() + 17 <= 96 {
        format!(
            "dependencies = [{}]",
            deps.iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        // 一行放不下就每行一个：读者以后要手改的时候，一行一个最好动。
        format!(
            "dependencies = [\n{}\n]",
            deps.iter()
                .map(|d| format!("    \"{d}\","))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let mut out = String::new();
    let mut lines = text.lines().peekable();
    let mut section = String::new();
    let mut replaced = false;
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name.trim().to_string();
        }
        let is_deps = section == "project"
            && trimmed
                .split_once('=')
                .is_some_and(|(left, _)| left.trim() == "dependencies");
        if !is_deps {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // 跨行的数组要把后面几行一起吃掉，否则会留下半截 ] 在文件里。
        if !trimmed.contains(']') {
            for rest in lines.by_ref() {
                if rest.contains(']') {
                    break;
                }
            }
        }
        out.push_str(&rendered);
        out.push('\n');
        replaced = true;
    }

    if !replaced {
        return Err(Error::new(
            Code::BadManifest,
            "vkx.toml 的 [project] 里没有 dependencies 字段",
            "补一行：dependencies = []",
        ));
    }
    crate::fs::write(&manifest, &out)
}

pub fn add(project: &Project, name: &str) -> Result<u8> {
    let key = canonical(name)?;
    if project.dependencies.iter().any(|d| d == key) {
        ui::info(&format!("{key} 已经在 dependencies 里了。"));
        return Ok(0);
    }
    // 按表的顺序重排：被依赖的要排在前面，生成 CMakeLists 时
    // 后面的 find_package 才找得到前面的。
    let mut want: Vec<&str> = project.dependencies.iter().map(String::as_str).collect();
    want.push(key);
    let ordered: Vec<&str> = DEPENDENCIES
        .iter()
        .filter(|d| want.contains(&d.name))
        .map(|d| d.name)
        .collect();
    write_dependencies(project, &ordered)?;
    ui::step(&format!("已加入 {key}"));
    ui::info("下次 vkx build 会把它链进来。");
    Ok(0)
}

pub fn remove(project: &Project, name: &str) -> Result<u8> {
    let key = canonical(name)?;
    if !project.dependencies.iter().any(|d| d == key) {
        ui::info(&format!("{key} 本来就不在 dependencies 里。"));
        return Ok(0);
    }
    let ordered: Vec<&str> = DEPENDENCIES
        .iter()
        .filter(|d| d.name != key && project.dependencies.iter().any(|n| n == d.name))
        .map(|d| d.name)
        .collect();
    write_dependencies(project, &ordered)?;
    ui::step(&format!("已移除 {key}"));
    Ok(0)
}

pub fn list(project: &Project) -> Result<u8> {
    ui::step("依赖（vkx.toml 的 dependencies）");
    for dep in DEPENDENCIES {
        let on = project.dependencies.iter().any(|d| d == dep.name);
        let mark = if on { "●" } else { "○" };
        ui::info(&format!("{mark} {:<22} {}", dep.name, dep.about));
    }
    ui::info("");
    ui::info("● 已启用   ○ 未启用。用 `vkx add <名字>` / `vkx remove <名字>` 改。");
    ui::info("全部是预编译好的，开关只影响链接和头文件，不影响构建时间。");
    Ok(0)
}
