//! `vkx doctor` —— 逐项检查环境，把「缺什么、怎么补」一次说清。

use crate::error::Result;
use crate::{toolchain, ui};

fn line(ok: bool, name: &str, detail: &str) {
    let mark = if ok { "有" } else { "缺" };
    ui::info(&format!("[{mark}] {name:<16} {detail}"));
}

pub fn run() -> Result<u8> {
    let mut missing = 0;

    // 逐条列出安装包里该有的东西，装没装、缺哪些一次看完。
    let host = crate::install::host()?;
    let entries: Vec<_> = crate::sdk::entries(host).collect();
    ui::step(&format!("开发平台 {}", host.name()));

    ui::step("SDK 组件");
    for e in &entries {
        let ok = crate::install::installed(e);
        if !ok {
            missing += 1;
        }
        line(ok, e.name, e.about);
    }

    ui::step("vkx 装不了、要你自己装的");
    let xcode = toolchain::xcodebuild().is_some();
    if cfg!(target_os = "macos") {
        line(xcode, "Xcode", "iOS 构建需要；Apple 不允许第三方再分发");
    }

    if missing > 0 {
        ui::step("怎么补");
        ui::info(&format!("vkx install vkx-{}.zip", host.name()));
        return Ok(1);
    }
    ui::step("组件都齐了");
    Ok(0)
}
