//! `vkx fetch` —— 从镜像取 SDK 包，而且只取需要的那几段。
//!
//! # 为什么不是整包下载
//!
//! 一个平台的 SDK 包里什么都有：工具、预编译的库、Vulkan、Android 的 NDK。
//! 只想跑桌面的人没必要为那几 GB 的 Android 部分等。
//!
//! # 为什么是「多个 .tar.gz 首尾相接」而不是一个 zip
//!
//! zip 的中央目录在文件末尾，取中间一段得到的字节不是合法 zip，还得自己实现
//! 解压。而 gzip 流是可以首尾相接的（多成员 gzip，标准允许），于是：
//!
//! - 打包时每个组件各压一个 .tar.gz，然后按顺序拼成一个文件
//! - 每一段单独拿出来仍然是合法的 .tar.gz，`tar xzf` 直接能解
//! - 清单记下每段的偏移、长度和 sha256
//!
//! 取一个组件 = 一次 HTTP Range 请求 + 一次校验 + 一次解压。
//!
//! # 为什么用 curl 而不是内建 HTTP
//!
//! 装 vkx 本来就要 curl（安装脚本用它下载 vkx 自己），而内建一套 TLS 会让这个
//! 二进制大一个数量级。curl 自带 --range、断点续传和进度条。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Code, Context, Error, Result};
use crate::ui;

/// 默认镜像，可用 VKX_MIRROR 覆盖。
const DEFAULT_MIRROR: &str = "https://yinli.tech/file";

pub fn mirror() -> String {
    std::env::var("VKX_MIRROR").unwrap_or_else(|_| DEFAULT_MIRROR.to_string())
}

/// SDK 根目录：解开的组件都在这下面。
pub fn sdk_dir() -> PathBuf {
    home().join(".vkx/sdk")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 本机对应哪个平台的包。
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", _) => "macos-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", _) => "linux-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", _) => "windows-x64",
        (os, arch) => {
            let _ = (os, arch);
            "unknown"
        }
    }
}

/// 清单里的一段。
pub struct Component {
    pub name: String,
    pub offset: u64,
    pub length: u64,
    pub sha256: String,
    pub about: String,
}

/// 清单是一行一条的文本，不引 JSON 库：
///
/// ```text
/// pack sdk-macos-arm64.pack
/// component toolchain 0 12582912 <sha256> cmake/ninja/slangc
/// component libs 12582912 19922944 <sha256> 预编译的 C 库和头文件
/// ```
pub struct Manifest {
    pub pack: String,
    pub components: Vec<Component>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let mut pack = String::new();
        let mut components = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(6, ' ');
            match parts.next() {
                Some("pack") => pack = parts.next().unwrap_or_default().to_string(),
                Some("component") => {
                    let name = parts.next().unwrap_or_default().to_string();
                    let offset = parse_num(parts.next(), &name, "offset")?;
                    let length = parse_num(parts.next(), &name, "length")?;
                    let sha256 = parts.next().unwrap_or_default().to_string();
                    let about = parts.next().unwrap_or_default().to_string();
                    components.push(Component {
                        name,
                        offset,
                        length,
                        sha256,
                        about,
                    });
                }
                _ => continue,
            }
        }
        if pack.is_empty() {
            return Err(Error::new(
                Code::MissingComponent,
                "清单里没有 pack 那一行",
                "镜像可能不完整，换一个站点：VKX_MIRROR=<地址> vkx fetch",
            ));
        }
        Ok(Self { pack, components })
    }
}

fn parse_num(value: Option<&str>, component: &str, field: &str) -> Result<u64> {
    value.and_then(|v| v.parse().ok()).ok_or_else(|| {
        Error::new(
            Code::MissingComponent,
            format!("清单里 {component} 的 {field} 不是数字"),
            "镜像可能不完整，换一个站点：VKX_MIRROR=<地址> vkx fetch",
        )
    })
}

fn curl() -> Result<PathBuf> {
    which("curl").ok_or_else(|| {
        Error::new(
            Code::Environment,
            "找不到 curl",
            "macOS 和 Windows 10 以上自带；Linux 上装一下：apt install curl / dnf install curl",
        )
    })
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// 下载 URL 的一段字节。`length` 为 0 表示整个文件。
fn download(url: &str, range: Option<(u64, u64)>, to: &Path, label: &str) -> Result<()> {
    let curl = curl()?;
    if let Some(parent) = to.parent() {
        crate::fs::create_dir_all(parent)?;
    }
    let mut command = Command::new(&curl);
    command.arg("-fL").arg("--progress-bar");
    if let Some((offset, length)) = range {
        command
            .arg("--range")
            .arg(format!("{}-{}", offset, offset + length - 1));
    }
    command.arg("-o").arg(to).arg(url);

    ui::step(&format!("取 {label}"));
    let status = command
        .status()
        .context(Code::Environment, "运行 curl", "确认 curl 可执行")?;
    if !status.success() {
        return Err(Error::new(
            Code::MissingComponent,
            format!("下载失败：{url}"),
            "确认网络可达；国内网络不通时可以换站点：VKX_MIRROR=<地址> vkx fetch",
        )
        .hint("站点必须支持 HTTP Range，否则取不了单个组件"));
    }
    Ok(())
}

fn sha256_of(path: &Path) -> Result<String> {
    let (program, args): (&str, Vec<&str>) = if which("sha256sum").is_some() {
        ("sha256sum", vec![])
    } else if which("shasum").is_some() {
        ("shasum", vec!["-a", "256"])
    } else {
        return Err(Error::new(
            Code::Environment,
            "找不到 sha256sum 或 shasum",
            "macOS 自带 shasum；Linux 上装 coreutils",
        ));
    };
    let output = Command::new(program)
        .args(&args)
        .arg(path)
        .output()
        .context(Code::Environment, format!("运行 {program}"), "确认它可执行")?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string())
}

/// 取清单。它很小（几百字节），每次都重新取，保证和镜像一致。
pub fn manifest() -> Result<Manifest> {
    let platform = platform();
    let url = format!("{}/sdk/{platform}/manifest.txt", mirror());
    let tmp = sdk_dir().join(".manifest.txt");
    download(&url, None, &tmp, &format!("{platform} 的清单"))?;
    let text = crate::fs::read_to_string(&tmp)?;
    Manifest::parse(&text)
}

/// 某个组件是不是已经装好了。
pub fn installed(name: &str) -> bool {
    sdk_dir().join(name).is_dir()
}

/// 取一个组件：Range 下载 → 校验 sha256 → 原子解压到位。
pub fn fetch_component(manifest: &Manifest, component: &Component) -> Result<()> {
    let target = sdk_dir().join(&component.name);
    if target.is_dir() {
        ui::info(&format!("{} 已经在了，跳过。", component.name));
        return Ok(());
    }

    let mb = component.length as f64 / 1_048_576.0;
    let url = format!("{}/sdk/{}/{}", mirror(), platform(), manifest.pack);
    let cache = home()
        .join(".vkx/cache")
        .join(format!("{}.tar.gz", component.name));

    download(
        &url,
        Some((component.offset, component.length)),
        &cache,
        &format!("{} ({mb:.0} MB) —— {}", component.name, component.about),
    )?;

    // 服务器不支持 Range 时会把整个包发回来，先按大小拦一道——
    // 这比让 sha256 失败更能说清问题出在哪。
    let got_size = std::fs::metadata(&cache)
        .context(Code::Io, "查看下载的文件", "确认 ~/.vkx/cache 可写")?
        .len();
    if got_size != component.length {
        crate::fs::remove_file(&cache)?;
        return Err(Error::new(
            Code::MissingComponent,
            format!(
                "取 {} 时要 {} 字节，服务器给了 {got_size} 字节",
                component.name, component.length
            ),
            "这个站点不支持 HTTP Range，没法只取一段。换一个支持的站点：VKX_MIRROR=<地址>",
        )
        .hint("Caddy、nginx 的静态文件服务默认都支持；Python 的 http.server 不支持"));
    }

    let got = sha256_of(&cache)?;
    if !component.sha256.is_empty() && got != component.sha256 {
        crate::fs::remove_file(&cache)?;
        return Err(Error::new(
            Code::MissingComponent,
            format!("{} 校验不通过", component.name),
            "重跑一次 vkx fetch；反复失败说明镜像上的包和清单对不上，请反馈",
        ));
    }

    // 先解到临时目录再改名，中途失败不会留下半个组件。
    let staging = sdk_dir().join(format!(".{}.tmp", component.name));
    crate::fs::remove_dir_all(&staging)?;
    crate::fs::create_dir_all(&staging)?;
    let status = Command::new("tar")
        .arg("xzf")
        .arg(&cache)
        .arg("-C")
        .arg(&staging)
        .status()
        .context(Code::Environment, "运行 tar", "确认系统里有 tar")?;
    if !status.success() {
        crate::fs::remove_dir_all(&staging)?;
        return Err(Error::new(
            Code::MissingComponent,
            format!("{} 解压失败", component.name),
            "删掉 ~/.vkx/cache 后重试：vkx clean --cache && vkx fetch",
        ));
    }
    std::fs::rename(&staging, &target).context(
        Code::Io,
        format!("把 {} 移到位", component.name),
        "确认 ~/.vkx 可写",
    )?;
    crate::fs::remove_file(&cache)?;
    ui::info(&format!("{} 装好了。", component.name));
    Ok(())
}

/// 桌面构建默认要哪些组件。Android 那几 GB 只有显式要才取。
const DESKTOP: &[&str] = &["toolchain", "libs", "vulkan"];

pub fn run(component: Option<&str>, all: bool) -> Result<u8> {
    let manifest = manifest()?;
    let wanted: Vec<&Component> = if all {
        manifest.components.iter().collect()
    } else if let Some(name) = component {
        let found = manifest
            .components
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| {
                let names: Vec<&str> = manifest
                    .components
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect();
                Error::new(
                    Code::MissingComponent,
                    format!("清单里没有叫 `{name}` 的组件"),
                    format!("可选：{}", names.join("、")),
                )
            })?;
        vec![found]
    } else {
        manifest
            .components
            .iter()
            .filter(|c| DESKTOP.contains(&c.name.as_str()))
            .collect()
    };

    for component in wanted {
        fetch_component(&manifest, component)?;
    }
    Ok(0)
}
