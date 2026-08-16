mod builder;
mod dist;
mod error;
mod mobile;
mod project;
mod prompt;
mod scaffold;
mod signing;
mod toolchain;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use builder::Profile;
use error::{Error, Result};
use project::Project;

#[derive(Parser)]
#[command(
    name = "vkx",
    version,
    about = "Vulkan + SDL3 跨平台游戏工程脚手架",
    long_about = "vkx 负责脚手架、工具链和构建，让你把注意力放在游戏本身。\n\
                  支持 Windows / macOS / Linux / Android / iOS。"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 新建一个 Vulkan + SDL3 工程
    New {
        /// 工程名（同时是可执行文件名）；不给就交互式询问
        name: Option<String>,
        /// 生成到指定目录，默认是 ./<name>
        #[arg(long)]
        path: Option<PathBuf>,
        /// Android / iOS 的包名；不给就交互式询问
        #[arg(long)]
        package_id: Option<String>,
    },
    /// 编译当前工程
    Build {
        /// 用 Release 配置编译
        #[arg(long)]
        release: bool,
        /// 目标平台，默认是本机桌面
        #[arg(long, value_enum, default_value_t = Target::Desktop)]
        target: Target,
    },
    /// 编译并运行当前工程
    Run {
        #[arg(long)]
        release: bool,
        /// 目标平台，默认是本机桌面
        #[arg(long, value_enum, default_value_t = Target::Desktop)]
        target: Target,
        /// `--` 之后的参数原样传给游戏（仅桌面）
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// 打出可以直接分发的安装包
    Dist {
        /// 目标平台，默认是本机桌面
        #[arg(long, value_enum, default_value_t = Target::Desktop)]
        target: Target,
    },
    /// 删除构建产物
    Clean,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            ui::report(&error);
            ExitCode::FAILURE
        }
    }
}

fn dispatch(cli: Cli) -> Result<u8> {
    match cli.command {
        Command::New { name, path, package_id } => {
            // 两项都是必需的：命令行上没给的，问用户要。
            let name = resolve_name(name, path.as_deref())?;
            let package_id = resolve_package_id(package_id, &name)?;

            let options = scaffold::NewOptions { name, path, package_id };
            let root = scaffold::create(&options)?;
            // 顺手把 Android 的 release 签名密钥备好，工程开箱就能出签名包。
            signing::setup_for_new_project(&root, &options.package_id);
            let relative = pretty_path(&root);

            eprintln!();
            eprintln!("工程已创建：{}", ui::bold(&relative));
            eprintln!();
            eprintln!("  cd {relative}");
            eprintln!("  vkx run");
            eprintln!();
            eprintln!("{}", ui::dim("首次构建会拉取并编译 SDL3 等依赖，需要几分钟。"));
            Ok(0)
        }
        Command::Build { release, target } => {
            let project = current_project()?;
            let profile = profile(release);
            let artifact = match target {
                Target::Desktop => builder::build(&project, profile)?,
                Target::Android => mobile::build_android(&project, profile)?,
                Target::Ios => mobile::build_ios(&project, profile, false)?,
                Target::IosDevice => mobile::build_ios(&project, profile, true)?,
            };
            eprintln!();
            eprintln!("产物：{}", ui::bold(&pretty_path(&artifact)));
            Ok(0)
        }
        Command::Run { release, target, args } => {
            let project = current_project()?;
            let profile = profile(release);
            let code = match target {
                Target::Desktop => builder::run(&project, profile, &args)?,
                Target::Android => mobile::run_android(&project, profile)?,
                Target::Ios => mobile::run_ios(&project, profile, false)?,
                Target::IosDevice => mobile::run_ios(&project, profile, true)?,
            };
            Ok(u8::try_from(code).unwrap_or(1))
        }
        Command::Dist { target } => {
            let project = current_project()?;
            let outputs = match target {
                Target::Desktop => vec![dist::dist_desktop(&project)?],
                Target::Android => dist::dist_android(&project)?,
                Target::Ios | Target::IosDevice => vec![dist::dist_ios(&project)?],
            };

            eprintln!();
            eprintln!("分发包：");
            for output in &outputs {
                eprintln!("  {}", ui::bold(&pretty_path(output)));
            }
            Ok(0)
        }
        Command::Clean => {
            let project = current_project()?;
            let build_dir = project.root.join("build");
            if build_dir.exists() {
                std::fs::remove_dir_all(&build_dir)?;
                ui::step(&format!("已删除 {}", pretty_path(&build_dir)));
            } else {
                ui::info("没有构建产物需要清理。");
            }
            Ok(0)
        }
    }
}

/// 取工程名：命令行给了就用，没给就问；两种情况都会校验，
/// 并顺带确认目标目录可用（免得填完包名才发现目录被占）。
fn resolve_name(name: Option<String>, path: Option<&std::path::Path>) -> Result<String> {
    let check = |candidate: &str| -> Result<()> {
        project::validate_name(candidate)?;
        scaffold::ensure_available(&scaffold::target_root(candidate, path)?)
    };

    match name {
        Some(name) => {
            check(&name)?;
            Ok(name)
        }
        None => {
            require_interactive("工程名")?;
            eprintln!();
            prompt::ask("工程名", Some("mygame"), check)
        }
    }
}

/// 取包名：命令行给了就用，没给就以 com.example.<工程名> 为默认值询问。
fn resolve_package_id(package_id: Option<String>, name: &str) -> Result<String> {
    match package_id {
        Some(package_id) => {
            project::validate_package_id(&package_id)?;
            Ok(package_id)
        }
        None => {
            require_interactive("包名")?;
            let default = scaffold::default_package_id(name);
            let answer = prompt::ask(
                "包名 (Android / iOS)",
                Some(&default),
                project::validate_package_id,
            )?;
            eprintln!();
            Ok(answer)
        }
    }
}

fn require_interactive(what: &str) -> Result<()> {
    if prompt::interactive() {
        return Ok(());
    }
    Err(Error::new(format!("没有提供{what}，当前又不是交互式终端"))
        .hint("非交互环境下请写全：vkx new <工程名> --package-id <包名>"))
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Target {
    /// 本机桌面（Windows / macOS / Linux）
    Desktop,
    /// Android 设备或模拟器
    Android,
    /// iOS 模拟器
    Ios,
    /// iOS 真机（需要开发者证书）
    IosDevice,
}

fn current_project() -> Result<Project> {
    Project::discover(&std::env::current_dir()?)
}

fn profile(release: bool) -> Profile {
    if release { Profile::Release } else { Profile::Debug }
}

/// 尽量按相对当前目录显示路径，输出短一点。
fn pretty_path(path: &std::path::Path) -> String {
    let Ok(current) = std::env::current_dir() else {
        return path.display().to_string();
    };
    match path.strip_prefix(&current) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ => path.display().to_string(),
    }
}
