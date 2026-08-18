//! `vkx help <专题|错误码>` —— 把长篇说明放在这里，短消息保持一行。

use clap::Command as ClapCommand;

use crate::error::{Code, Context, Result};
use crate::ui;

/// 所有错误码，用来支持 `vkx help E0003` 和列表。
const CODES: &[Code] = &[
    Code::NotAProject,
    Code::BadManifest,
    Code::MissingComponent,
    Code::CommandFailed,
    Code::Io,
    Code::Environment,
    Code::Usage,
];

/// 专题：名字、一句话简介、正文。
const TOPICS: &[(&str, &str, &str)] = &[
    (
        "manifest",
        "vkx.toml 有哪些字段",
        "工程根目录下的 vkx.toml 是唯一需要你编辑的配置文件。\n\n\
         [project]  name / package_id / version\n\
         [libs]     要从源码编的库开关\n\
         [ios]      development_team\n\n\
         CMakeLists.txt 由 vkx 生成到 target/，不要手改。",
    ),
    (
        "toolchain",
        "工具链装在哪、怎么清",
        "全部组件都在 ~/.vkx 下：\n\n\
         bin/      vkx 自己\n\
         sdk/      按需下载的组件（工具、库、Vulkan、Android）\n\
         cache/    下载缓存，可随时删\n\n\
         删掉整个 ~/.vkx 就等于卸载干净。",
    ),
    (
        "ios",
        "从零到真机的完整路径",
        "1. 装完整版 Xcode（不是命令行工具），并在 Settings → Components 里装 iOS 平台\n\
         2. sudo xcode-select -s /Applications/Xcode.app\n\
         3. 模拟器：vkx run --target ios\n\
         4. 真机：在 vkx.toml 的 [ios] 里填 development_team，然后 vkx dist --target ios-device",
    ),
    (
        "fetch",
        "工具链是怎么下载的",
        "每个平台一个 SDK 包，里面按组件分段：toolchain / libs / vulkan / android。\n\n\
         vkx fetch                取桌面构建需要的那几段\n\
         vkx fetch --component android   出安卓包时才取那几 GB\n\
         vkx fetch --all\n\n\
         用 HTTP Range 只下需要的字节，所以不想做安卓的人不必等那几 GB。\n\
         站点必须支持 Range；不支持时 vkx 会当场说清而不是默默下整包。",
    ),
    (
        "mirror",
        "换下载站点、自建镜像",
        "默认站点写在 vkx 里。临时换用环境变量：\n\n\
         VKX_MIRROR=https://example.com/vkx vkx fetch\n\n\
         自建只需要一个支持 HTTP Range 的静态文件服务。",
    ),
];

pub fn run(topic: Option<&str>, cli: &mut ClapCommand) -> Result<u8> {
    const TTY: &str = "终端异常时可以把输出重定向到文件再看";
    let Some(topic) = topic else {
        cli.print_help().context(Code::Io, "打印帮助", TTY)?;
        println!();
        list();
        return Ok(0);
    };

    // 是命令名就转发给 clap 自己的帮助
    if let Some(sub) = cli.find_subcommand_mut(topic) {
        sub.print_help().context(Code::Io, "打印帮助", TTY)?;
        println!();
        return Ok(0);
    }

    let wanted = topic.to_ascii_uppercase();
    if let Some(code) = CODES.iter().find(|c| c.id() == wanted) {
        ui::step(&format!("{} {}", code.id(), first_line(code.explain())));
        for line in code.explain().lines().skip(1) {
            ui::info(line);
        }
        return Ok(0);
    }

    if let Some((name, brief, body)) = TOPICS.iter().find(|(n, _, _)| *n == topic) {
        ui::step(&format!("{name} —— {brief}"));
        for line in body.lines() {
            ui::info(line.trim_start());
        }
        return Ok(0);
    }

    ui::warn(&format!("没有叫 `{topic}` 的专题或错误码。"));
    list();
    Ok(1)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn list() {
    ui::step("专题");
    for (name, brief, _) in TOPICS {
        ui::info(&format!("{:<12} {brief}", name));
    }
    ui::step("错误码");
    for code in CODES {
        ui::info(&format!("{:<12} {}", code.id(), first_line(code.explain())));
    }
    ui::info("");
    ui::info("用法：vkx help <专题>   或   vkx help E0003");
}
