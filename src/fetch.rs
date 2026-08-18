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
//! # 解压和校验都在进程内做
//!
//! 不调用宿主机的 tar 和 sha256sum：Windows 上 `sha256sum` 根本不存在，而
//! System32 里那个 bsdtar 是 3.3.2（2017 年的 libarchive），不支持 zstd。
//! 依赖「机器上碰巧装了什么」的工具不叫自带电池。
//!
//! 所以解压用纯 Rust 的 tar + flate2 + ruzstd，校验用 sha2。压缩格式按魔数
//! 自动认，包换成 zstd 也不用改代码。
//!
//! # HTTP 也在进程内
//!
//! 不调 curl。vkx 不依赖宿主机 PATH 上碰巧有什么——那样一来行为取决于读者的
//! 机器装了什么，出问题很难远程判断。
//!
//! 证书优先用操作系统的验证（`platform-verifier` 走的是系统 API，不是外部
//! 二进制），这样企业网里的 TLS 中间人也能正常工作；系统那条路走不通时回退
//! 到内置的 webpki-roots。

use std::path::{Path, PathBuf};

use crate::error::{Code, Context, Error, Result};
use crate::ui;

/// 默认镜像，可用 VKX_MIRROR 覆盖。
const DEFAULT_MIRROR: &str = "https://yinli.tech/file";

pub fn mirror() -> String {
    std::env::var("VKX_MIRROR").unwrap_or_else(|_| DEFAULT_MIRROR.to_string())
}

/// SDK 根目录：解开的组件都在这下面。
///
/// 走 toolchain::vkx_home()，不要自己再算一遍 HOME——那边认 VKX_HOME，
/// 这边不认的话，设了 VKX_HOME 就会变成「装到一个地方、去另一个地方找」。
pub fn sdk_dir() -> PathBuf {
    crate::toolchain::vkx_home().join("sdk")
}

/// 本机对应哪个平台的包。
pub const PLATFORMS: &[&str] = &[
    "macos-arm64",
    "macos-x64",
    "linux-arm64",
    "linux-x64",
    "windows-arm64",
    "windows-x64",
];

/// 本机平台，也就是去镜像上找哪一个 SDK 包。
///
/// `VKX_PLATFORM` 可以覆盖它。这不是给读者用的——是给我们发包时用的：
/// 交叉编出来的包（比如在 arm64 机器上编的 macos-x64）必须能在编它的那台机器上
/// 自检，否则「包是不是拼对了」这个问题只能等读者去发现。
pub fn platform() -> &'static str {
    static OVERRIDE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let chosen = OVERRIDE.get_or_init(|| std::env::var("VKX_PLATFORM").ok());
    if let Some(name) = chosen {
        if let Some(known) = PLATFORMS.iter().find(|p| *p == name) {
            return known;
        }
        // 不认识就当没设——真去下载时会报「取不到清单」，那个错更难查，
        // 所以这里先把话说清楚。
        eprintln!(
            "警告：VKX_PLATFORM={name} 不是已知平台，忽略。可选：{}",
            PLATFORMS.join("、")
        );
    }
    host_platform()
}

fn host_platform() -> &'static str {
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

/// 发一个 GET，可选地只要其中一段字节，边下边写文件并显示进度。
///
/// `range` 是 (起始偏移, 长度)。给了就发 `Range: bytes=a-b`，服务器不认时
/// 会把整个文件发回来——调用方按收到的字节数就能发现。
pub fn download_to(url: &str, to: &Path) -> Result<()> {
    download(url, None, to, "新版本 vkx")
}

fn download(url: &str, range: Option<(u64, u64)>, to: &Path, label: &str) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = to.parent() {
        crate::fs::create_dir_all(parent)?;
    }

    ui::step(&format!("取 {label}"));

    let mut request = ureq::get(url);
    if let Some((offset, length)) = range {
        request = request.header("Range", format!("bytes={}-{}", offset, offset + length - 1));
    }
    let response = request.call().map_err(|e| {
        Error::new(
            Code::MissingComponent,
            format!("请求失败：{url}\n  {e}"),
            "确认网络可达；国内网络不通时换站点：VKX_MIRROR=<地址> vkx fetch",
        )
    })?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(to).context(
        Code::Io,
        format!("创建 {}", to.display()),
        "确认 ~/.vkx 可写，且磁盘还有空间",
    )?;

    let mut buffer = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut last_shown = 0u64;
    loop {
        use std::io::Read;
        let read = reader.read(&mut buffer).context(
            Code::MissingComponent,
            "读取响应",
            "网络中断了，重跑一次 vkx fetch",
        )?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).context(
            Code::Io,
            format!("写入 {}", to.display()),
            "确认磁盘还有空间：vkx clean --cache 可以腾出一些",
        )?;
        done += read as u64;
        // 每 4 MB 报一次，别把终端刷爆
        if done - last_shown >= 4 << 20 {
            last_shown = done;
            match total {
                Some(total) if total > 0 => {
                    ui::progress(done, total);
                }
                _ => ui::info(&format!("  已下载 {:.0} MB", done as f64 / 1_048_576.0)),
            }
        }
    }
    if let Some(total) = total {
        ui::progress(done, total);
    }
    ui::progress_done();
    Ok(())
}

fn sha256_of(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).context(
        Code::Io,
        format!("打开 {}", path.display()),
        "确认 ~/.vkx/cache 可读",
    )?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        use std::io::Read;
        let read = file.read(&mut buffer).context(
            Code::Io,
            format!("读取 {}", path.display()),
            "确认磁盘没有故障；重跑 vkx fetch 会重新下载",
        )?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// 按魔数认压缩格式，解一层压缩后交给 tar。
///
/// gzip 是 `1f 8b`，zstd 是 `28 b5 2f fd`，都认不出就当没压缩的裸 tar。
/// 这样包换成别的压缩格式不用改代码，也不用管宿主机的 tar 支持什么。
fn unpack(archive: &Path, into: &Path) -> Result<()> {
    use std::io::Read;

    let mut head = [0u8; 4];
    {
        let mut probe = std::fs::File::open(archive).context(
            Code::Io,
            format!("打开 {}", archive.display()),
            "确认 ~/.vkx/cache 可读",
        )?;
        let _ = probe.read(&mut head);
    }

    let file = std::fs::File::open(archive).context(
        Code::Io,
        format!("打开 {}", archive.display()),
        "确认 ~/.vkx/cache 可读",
    )?;
    let reader = std::io::BufReader::new(file);

    let stream: Box<dyn Read> = if head[..2] == [0x1f, 0x8b] {
        Box::new(flate2::read::GzDecoder::new(reader))
    } else if head == [0x28, 0xb5, 0x2f, 0xfd] {
        Box::new(
            ruzstd::decoding::StreamingDecoder::new(reader).map_err(|e| {
                Error::new(
                    Code::MissingComponent,
                    format!("zstd 流读不了：{e}"),
                    "删掉缓存后重试：vkx clean --cache && vkx fetch",
                )
            })?,
        )
    } else {
        Box::new(reader)
    };

    tar::Archive::new(stream).unpack(into).context(
        Code::MissingComponent,
        "解包",
        "删掉缓存后重试：vkx clean --cache && vkx fetch",
    )
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
    let cache = crate::toolchain::vkx_home()
        .join("cache")
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
    if let Err(e) = unpack(&cache, &staging) {
        crate::fs::remove_dir_all(&staging)?;
        return Err(e);
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

/// 确保这几个组件都在本地，缺的取回来。
///
/// 和逐个调 run() 的区别是清单只读一次——否则每取一个组件就要往镜像上问一次
/// 清单，屏幕上刷出三遍「取 xxx 的清单」，看着像卡住了。
pub fn ensure(names: &[&str]) -> Result<()> {
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| !installed(name))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let manifest = manifest()?;
    for name in missing {
        let Some(component) = manifest.components.iter().find(|c| c.name == name) else {
            continue;
        };
        fetch_component(&manifest, component)?;
    }
    Ok(())
}

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
