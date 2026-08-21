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
         CMakeLists.txt 由 vkx 生成到 target/，不要手改。",
    ),
    (
        "toolchain",
        "工具链装在哪、怎么清",
        "全部组件都在 ~/.vkx 下：\n\n\
         bin/      vkx 自己\n\
         sdk/      从安装包装进来的组件（工具、库、Vulkan、Android）\n\
         cache/    下载缓存，可随时删\n\n\
         删掉整个 ~/.vkx 就等于卸载干净。",
    ),
    (
        "ios",
        "从零到真机的完整路径",
        "1. 装完整版 Xcode（不是命令行工具），并在 Settings → Components 里装 iOS 平台\n\
         2. sudo xcode-select -s /Applications/Xcode.app\n\
         3. 模拟器：vkx run --target ios\n\
         4. 真机：vkx build --target ios-device 生成 Xcode 工程，用 Xcode 打开\n\n\
         vkx 到「生成 Xcode 工程」为止。签名、连真机、Archive 上架都在 Xcode 里做——\n\
         那些事绑 Apple 账号，格式也跟着 Xcode 版本变，vkx 发出去就不再更新了。",
    ),
    (
        "install",
        "工具链是怎么装的",
        "vkx 不从网上取任何东西。它要的一切——cmake、ninja、slangc、Vulkan、\n\
         JDK、Gradle、Android SDK 和 NDK——都在离线安装包里，一个开发平台一个：\n\n\
         vkx install vkx-macos-arm64.zip\n\
         vkx install vkx-linux-x64.zip\n\
         vkx install vkx-windows-x64.zip\n\n\
         装到 ~/.vkx（VKX_HOME 可以改）。只补缺的：已经装好并且校验通过的直接跳过，\n\
         所以重复执行是安全的，中断之后再跑一次就接着装。\n\n\
         每一样在装之前都按 blake3 校验，对不上就不装、不留半成品。\n\
         `vkx doctor` 列出哪些装了哪些没装。想全部重装用 --force。",
    ),
    (
        "version",
        "为什么依赖不追版本",
        "一个 vkx 版本对应一套确定的依赖，版本号写死在二进制里，出 CVE 也不动。\n\n\
         好处是构建可复现：同一个 vkx 在任何机器、任何时候装出来的都是同一套东西，\n\
         不会因为上游发了新 patch 而变。想换依赖就换 vkx——没有别的路径能改动\n\
         你机器上装的是什么。",
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
