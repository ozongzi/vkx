mod builder;
mod deps;
mod dist;
mod doctor;
mod error;
mod fmt;
mod fs;
mod generate;
mod help;
mod install;
mod mobile;
mod payload;
mod project;
mod prompt;
mod scaffold;
mod sdk;
mod signing;
mod toolchain;
mod ui;

use crate::error::{Code, Context};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use builder::Profile;
use error::{Error, Result};
use project::Project;

#[derive(Parser)]
#[command(
    disable_help_subcommand = true,
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
    /// 按 .clang-format 格式化 src/ 下的源码
    Fmt {
        /// 只检查不修改：有文件不合格式就以非零码退出，给 CI 用
        #[arg(long)]
        check: bool,
    },
    /// 删除构建产物
    Clean,
    /// 从离线安装包补齐 SDK：vkx install vkx-<平台>.zip
    Install {
        /// 安装包路径
        bundle: std::path::PathBuf,
        /// 已经装好的也重装一遍
        #[arg(long)]
        force: bool,
        /// 不要改 PATH
        #[arg(long)]
        no_path: bool,
    },
    /// 检查环境，报告缺什么、怎么补
    Doctor,
    /// vkx 自身的维护
    #[command(subcommand)]
    #[command(name = "self")]
    Selfcmd(SelfCommand),
    /// 打开一个要从源码编译的库
    Add {
        /// 库名；`vkx deps` 列出可选项
        name: String,
    },
    /// 关掉一个之前打开的库
    Remove { name: String },
    /// 列出可用的库，以及哪些正在参与构建
    Deps,
    /// 展开某个错误码，或者某个专题的详细说明
    Help {
        /// 错误码（如 E0003）或专题名；不给就列出所有专题
        topic: Option<String>,
    },
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
        Command::New {
            name,
            path,
            package_id,
        } => {
            // 两项都是必需的：命令行上没给的，问用户要。
            let name = resolve_name(name, path.as_deref())?;
            let package_id = resolve_package_id(package_id, &name)?;

            let options = scaffold::NewOptions {
                name,
                path,
                package_id,
            };
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
            // 别在这儿吓唬人：SDL3 是 sdk/libs 里的预编译库，find_package 直接
            // 找到，既不出网也不编译。首次构建真正要编的只有 volk 那一个 .c、
            // 两个着色器和工程自己的源码，十几秒的事。
            eprintln!("{}", ui::dim("首次构建要编一遍着色器和工程源码，很快。"));
            Ok(0)
        }
        Command::Build { release, target } => {
            let project = current_project()?;
            let profile = profile(release);
            gate(target)?;
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
        Command::Run {
            release,
            target,
            args,
        } => {
            let project = current_project()?;
            let profile = profile(release);
            gate(target)?;
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
            gate(target)?;
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
        Command::Fmt { check } => {
            install::require_fmt()?;
            let project = current_project()?;
            fmt::run(&project, check)
        }
        Command::Install {
            bundle,
            force,
            no_path,
        } => {
            install::install_from(&bundle, force, !no_path)?;
            Ok(0)
        }
        Command::Doctor => doctor::run(),
        Command::Selfcmd(SelfCommand::Uninstall { yes }) => {
            install::uninstall(yes)?;
            Ok(0)
        }
        Command::Add { name } => {
            let project = current_project()?;
            deps::add(&project, &name)
        }
        Command::Remove { name } => {
            let project = current_project()?;
            deps::remove(&project, &name)
        }
        Command::Deps => {
            let project = current_project()?;
            deps::list(&project)
        }
        Command::Help { topic } => help::run(topic.as_deref(), &mut Cli::command()),
        Command::Clean => {
            let project = current_project()?;
            let build_dir = project.root.join("target");
            if build_dir.exists() {
                crate::fs::remove_dir_all(&build_dir)?;
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
    Err(Error::new(
        Code::Usage,
        format!("没有提供{what}，当前又不是交互式终端"),
        "非交互环境下请写全：vkx new <工程名> --package-id <包名>",
    ))
}

#[derive(Subcommand)]
enum SelfCommand {
    /// 删掉 ~/.vkx，把 vkx 装的东西全部卸干净
    Uninstall {
        /// 不问直接删
        #[arg(long)]
        yes: bool,
    },
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

/// 动手之前先看这个目标要的东西齐不齐，缺就直接报缺哪些。
fn gate(target: Target) -> Result<()> {
    match target {
        Target::Desktop => install::require_desktop(),
        Target::Android => install::require_android(),
        Target::Ios | Target::IosDevice => install::require_ios(),
    }
}

fn current_project() -> Result<Project> {
    Project::discover(&std::env::current_dir().context(
        Code::Io,
        "取当前目录",
        "确认当前目录还存在且有读权限",
    )?)
}

fn profile(release: bool) -> Profile {
    if release {
        Profile::Release
    } else {
        Profile::Debug
    }
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
