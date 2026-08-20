use std::io::IsTerminal;
use std::sync::OnceLock;

use crate::error::Error;

fn use_color() -> bool {
    static COLOR: OnceLock<bool> = OnceLock::new();
    *COLOR.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal())
}

fn paint(code: &str, text: &str) -> String {
    if use_color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    paint("1", text)
}

pub fn dim(text: &str) -> String {
    paint("2", text)
}

/// 一个大步骤的开始，例如「配置工程」「编译」。
pub fn step(text: &str) {
    eprintln!("{} {}", paint("1;32", "==>"), bold(text));
}

pub fn info(text: &str) {
    eprintln!("    {text}");
}

pub fn warn(text: &str) {
    eprintln!("{} {}", paint("1;33", "警告:"), text);
}

pub fn report(error: &Error) {
    let code = error.code.id();
    eprintln!();
    eprintln!(
        "{} {}",
        paint("1;31", &format!("错误[{code}]:")),
        error.message
    );
    for hint in &error.hints {
        eprintln!("{} {}", paint("1;36", "提示:"), hint);
    }
    eprintln!("{}", dim(&format!("      详细说明：vkx help {code}")));
    eprintln!();
}

/// 单行覆盖式进度条，带一句说明这是在干什么。
/// 不是终端时什么都不打，避免把日志刷满。
pub fn progress_labeled(label: &str, done: u64, total: u64) {
    use std::io::{IsTerminal, Write};
    if !std::io::stderr().is_terminal() {
        return;
    }
    let ratio = if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    };
    let width = 24;
    let filled = (ratio * width as f64).round() as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(width - filled);
    let _ = write!(
        std::io::stderr(),
        "\r    {label} {bar} {:>3.0}%  {:.0}/{:.0} MB   ",
        ratio * 100.0,
        done as f64 / 1_048_576.0,
        total as f64 / 1_048_576.0
    );
    let _ = std::io::stderr().flush();
}

/// 单行覆盖式进度条。不是终端时什么都不打，避免把日志刷满。
#[allow(dead_code)]
pub fn progress(done: u64, total: u64) {
    use std::io::{IsTerminal, Write};
    if !std::io::stderr().is_terminal() {
        return;
    }
    let ratio = (done as f64 / total as f64).clamp(0.0, 1.0);
    let width = 32;
    let filled = (ratio * width as f64).round() as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(width - filled);
    let _ = write!(
        std::io::stderr(),
        "\r    {bar} {:.0}%  {:.1}/{:.1} MB",
        ratio * 100.0,
        done as f64 / 1_048_576.0,
        total as f64 / 1_048_576.0
    );
    let _ = std::io::stderr().flush();
}

/// 进度条画完了，换行让后面的输出从头开始。
pub fn progress_done() {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!();
    }
}
