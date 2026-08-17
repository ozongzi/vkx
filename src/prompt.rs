use std::io::{IsTerminal, Write};

use crate::error::{Error, Result};
use crate::ui;

/// 只有 stdin 和 stderr 都连着终端时才能交互问答；
/// 管道里或 CI 上必须靠命令行参数把信息给全。
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// 问一个问题并读一行。
///
/// - 直接回车 -> 用 `default`（没有默认值就重问）
/// - 输入不合法 -> 打印原因后重问
/// - Ctrl-D / Ctrl-C -> 取消
pub fn ask(
    question: &str,
    default: Option<&str>,
    validate: impl Fn(&str) -> Result<()>,
) -> Result<String> {
    if !interactive() {
        return Err(Error::new("当前不是交互式终端，无法询问缺少的参数"));
    }

    loop {
        match default {
            Some(value) => eprint!(
                "  {} {} › ",
                ui::bold(question),
                ui::dim(&format!("({value})"))
            ),
            None => eprint!("  {} › ", ui::bold(question)),
        }
        std::io::stderr().flush()?;

        let mut line = String::new();
        let read = std::io::stdin().read_line(&mut line)?;
        if read == 0 {
            eprintln!();
            return Err(Error::new("输入已结束，已取消"));
        }

        let answer = line.trim();
        let answer = if answer.is_empty() {
            default.unwrap_or_default()
        } else {
            answer
        };

        if answer.is_empty() {
            ui::warn("不能为空");
            continue;
        }

        match validate(answer) {
            Ok(()) => return Ok(answer.to_string()),
            Err(error) => {
                ui::warn(&error.message);
                for hint in &error.hints {
                    ui::info(hint);
                }
            }
        }
    }
}
