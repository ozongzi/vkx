//! `vkx fmt` —— 按工程根目录的 .clang-format 格式化源码。
//!
//! 只碰 src/ 下的 C/C++ 源码。着色器（.slang）不归 clang-format 管，跳过。
//!
//! `--check` 模式不改文件，只报告哪些文件不合格式并以非零码退出，给 CI 用。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Code, Error, Result};
use crate::project::Project;
use crate::toolchain;
use crate::ui;

/// clang-format 认的扩展名。
const EXTENSIONS: &[&str] = &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"];

/// 收集 src/ 下所有该格式化的文件，按路径排序，输出才稳定。
fn collect(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let src = root.join("src");
    if !src.is_dir() {
        return Ok(files);
    }
    collect_into(&src, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_into(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs::read_dir(dir)? {
        let path = entry;
        if path.is_dir() {
            collect_into(&path, out)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && EXTENSIONS.contains(&ext)
        {
            out.push(path);
        }
    }
    Ok(())
}

/// 相对工程根显示，输出短一些。
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn run(project: &Project, check: bool) -> Result<u8> {
    let config = project.root.join(".clang-format");
    if !config.is_file() {
        return Err(Error::new(
            Code::Environment,
            "工程根目录下没有 .clang-format",
            "vkx new 生成的工程自带一份；手工建的工程需要自己补",
        ));
    }

    let clang_format = toolchain::clang_format()?;
    let files = collect(&project.root)?;
    if files.is_empty() {
        ui::info("src/ 下没有找到 C/C++ 源码。");
        return Ok(0);
    }

    if check {
        // --dry-run 配 --Werror：不合格式的文件会让 clang-format 以非零码退出，
        // 同时把具体位置打到 stderr 上。
        let mut offenders = Vec::new();
        for file in &files {
            // clang-format 会把每一处不合规的位置都打到 stderr 上，十几个文件下来
            // 是一屏幕噪音。这里只要「合不合格」这个结论，具体位置让用户跑
            // `vkx fmt` 直接改掉就好，所以把它的输出丢掉。
            let status = Command::new(&clang_format)
                .arg("--dry-run")
                .arg("--Werror")
                .arg(file)
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|e| {
                    Error::new(
                        Code::MissingComponent,
                        format!("无法运行 clang-format: {e}"),
                        "用 `vkx install vkx-<平台>.zip` 补齐工具链",
                    )
                })?;
            if !status.success() {
                offenders.push(file.clone());
            }
        }

        if offenders.is_empty() {
            ui::step(&format!("{} 个文件，格式都符合 .clang-format", files.len()));
            return Ok(0);
        }
        ui::step(&format!("{} 个文件需要格式化：", offenders.len()));
        for file in &offenders {
            ui::info(&relative(&project.root, file));
        }
        ui::info("运行 `vkx fmt` 就地修好。");
        return Ok(1);
    }

    // 就地格式化。clang-format 对没有变化的文件不会重写，所以不必自己比对。
    let mut command = Command::new(&clang_format);
    command.arg("-i").args(&files);
    toolchain::run(&mut command, "clang-format")?;

    ui::step(&format!("已格式化 {} 个文件", files.len()));
    Ok(0)
}
