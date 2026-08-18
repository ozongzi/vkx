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
