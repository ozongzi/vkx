//! `vkx doctor` —— 逐项检查环境，把「缺什么、怎么补」一次说清。

use crate::error::Result;
use crate::{fetch, toolchain, ui};

fn line(ok: bool, name: &str, detail: &str) {
    let mark = if ok { "有" } else { "缺" };
    ui::info(&format!("[{mark}] {name:<16} {detail}"));
}

pub fn run() -> Result<u8> {
    let mut missing = 0;

    ui::step(&format!("平台 {}", fetch::platform()));

    ui::step("工具链组件");
    for (name, about) in [
        ("toolchain", "cmake / ninja / slangc / clang-format"),
        ("libs", "预编译的 C 库和头文件"),
        ("vulkan", "loader、校验层"),
        ("android", "JDK / Gradle / SDK / NDK（出安卓包才需要）"),
    ] {
        let ok = fetch::installed(name);
        if !ok && name != "android" {
            missing += 1;
        }
        line(ok, name, about);
    }

    ui::step("vkx 装不了、要你自己装的");
    let xcode = toolchain::xcodebuild().is_some();
    if cfg!(target_os = "macos") {
        line(xcode, "Xcode", "iOS 构建需要；Apple 不允许第三方再分发");
    }

    if missing > 0 {
        ui::step("怎么补");
        ui::info("vkx fetch            取桌面构建需要的组件");
        ui::info("vkx fetch --all      连 Android 那几 GB 一起取");
        return Ok(1);
    }
    ui::step("桌面构建需要的东西都齐了");
    Ok(0)
}
