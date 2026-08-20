//! `vkx install`、`vkx doctor` 的安装侧，以及各命令跑之前的「东西齐不齐」检查。
//!
//! # 装的是什么
//!
//! 一个离线安装包（`vkx-<平台>.zip`）里带着这个平台要的全部依赖，原样是上游
//! 发的包，没有预先解开——解开要落到目标机器的文件系统上才对：符号链接、可执行
//! 位、大小写敏感与否，都是那台机器的事。
//!
//! # 「装好了」是怎么判定的
//!
//! 装完在 `~/.vkx/sdk/.installed/<组件>` 写一个戳，内容是安装包里那个文件的
//! blake3。判定「已安装」= 目标目录在 + 戳在 + 戳的值和二进制里硬编码的一致。
//!
//! 校验的是**来源**而不是装完那棵树。树哈希每跑一次命令都要过一遍几个 GB，
//! 不现实。这个取舍挡得住的是「装错版本」「装了一半」「包被换过」，挡不住
//! 「装完之后有人手动改了里面的文件」——那种情况 `vkx install --force` 重装。

use crate::error::{Code, Context, Error, Result};
use crate::sdk::{self, Entry, Format, Host};
use crate::toolchain::vkx_home;
use crate::ui;
use std::io::Read;
use std::path::{Path, PathBuf};

/// SDK 装在哪。
pub fn sdk_dir() -> PathBuf {
    vkx_home().join("sdk")
}

fn stamp_path(entry: &Entry) -> PathBuf {
    sdk_dir().join(".installed").join(entry.name)
}

/// 本机是哪个开发平台。不认识就直接报错——vkx 只支持三个。
pub fn host() -> Result<Host> {
    Host::detect().ok_or_else(|| {
        Error::new(
            Code::Usage,
            format!(
                "vkx 不支持这台机器（{} {}）",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            format!(
                "支持的开发平台只有：{}",
                Host::ALL
                    .iter()
                    .map(|h| h.name())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        )
    })
}

/// 这一条装好了没有。
pub fn installed(entry: &Entry) -> bool {
    if !sdk_dir().join(entry.dest).exists() {
        return false;
    }
    match std::fs::read_to_string(stamp_path(entry)) {
        Ok(stamp) => stamp.trim() == entry.blake3,
        Err(_) => false,
    }
}

/// 缺哪些。给各命令跑之前做检查用。
pub fn missing<'a>(names: &[&str], host: Host) -> Vec<&'a Entry> {
    sdk::entries(host)
        .filter(|e| names.contains(&e.name) && !installed(e))
        .collect()
}

/// 某个命令要用的组件齐不齐，不齐就报出缺哪些。
pub fn require(names: &[&str], what: &str) -> Result<()> {
    let host = host()?;

    // 要的名字表里根本没有，说明是 vkx 自己写错了——不能当成「齐了」放过去。
    // 放过去的后果是这里通过、构建时才炸在找不到某个可执行文件上，那个错更难查。
    let unknown: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| !sdk::entries(host).any(|e| e.name == *n))
        .collect();
    if !unknown.is_empty() {
        return Err(Error::new(
            Code::Usage,
            format!("vkx 内部错误：依赖表里没有 {}", unknown.join("、")),
            "这是 vkx 的 bug，请连同这条信息报给我们",
        ));
    }

    let gone = missing(names, host);
    if gone.is_empty() {
        return Ok(());
    }
    let list = gone
        .iter()
        .map(|e| format!("  {} —— {}", e.name, e.about))
        .collect::<Vec<_>>()
        .join("\n");
    Err(Error::new(
        Code::MissingComponent,
        format!("{what} 还缺这些东西：\n{list}"),
        format!(
            "从离线安装包补齐：vkx install vkx-{}.zip",
            host.name()
        ),
    ))
}

fn blake3_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).context(
        Code::Io,
        format!("打开 {}", path.display()),
        "确认文件还在、可读",
    )?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .context(Code::Io, "读取", "确认磁盘可读")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// 解一个包到目录里。zip 走 zip 库，其余按魔数认压缩后交给 tar。
///
/// tar 那条路符号链接和权限位是库自己处理的；zip 没有统一约定，所以要自己
/// 认符号链接和 unix 权限——NDK 里 `clang++ -> clang` 这类链接有几百个，
/// 当成普通文件复制会让包大出好几百 MB，而且 clang 找不到自己的名字。
/// 读了多少字节就报多少。解包没法预先知道会出来多少文件，但输入流有多长是
/// 知道的，拿它当进度最准，而且 zip 和 tar 两条路能用同一个尺子。
struct Counting<R> {
    inner: R,
    done: u64,
    total: u64,
    last: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done += n as u64;
        // 每 4 MB 刷一次就够了，刷太勤反而拖慢解包。
        if self.done - self.last > 4 << 20 {
            self.last = self.done;
            ui::progress_labeled("解包", self.done, self.total);
        }
        Ok(n)
    }
}

fn unpack(archive: &Path, into: &Path, format: Format) -> Result<()> {
    crate::fs::create_dir_all(into)?;
    match format {
        Format::Zip => unpack_zip(archive, into),
        _ => {
            let file = std::fs::File::open(archive).context(
                Code::Io,
                format!("打开 {}", archive.display()),
                "确认安装包完整",
            )?;
            let total = file.metadata().map(|m| m.len()).unwrap_or(0);
            let reader = Counting {
                inner: std::io::BufReader::new(file),
                done: 0,
                total,
                last: 0,
            };
            let stream: Box<dyn Read> = match format {
                Format::TarGz => Box::new(flate2::read::GzDecoder::new(reader)),
                Format::TarZst => Box::new(
                    ruzstd::decoding::StreamingDecoder::new(reader).map_err(|e| {
                        Error::new(Code::MissingComponent, format!("zstd 流读不了：{e}"), "安装包可能损坏，重新下载")
                    })?,
                ),
                _ => Box::new(reader),
            };
            let r = tar::Archive::new(stream)
                .unpack(into)
                .context(Code::MissingComponent, "解包", "安装包可能损坏，重新下载");
            ui::progress_done();
            r
        }
    }
}

fn unpack_zip(archive: &Path, into: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).context(
        Code::Io,
        format!("打开 {}", archive.display()),
        "确认安装包完整",
    )?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| Error::new(Code::MissingComponent, format!("读 zip 失败：{e}"), "安装包可能损坏"))?;

    let total = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    let mut seen = 0u64;
    let mut last = 0u64;

    for i in 0..zip.len() {
        let mut item = zip
            .by_index(i)
            .map_err(|e| Error::new(Code::MissingComponent, format!("读 zip 第 {i} 项失败：{e}"), "安装包可能损坏"))?;
        // enclosed_name 会挡住 `../` 这种想跑到目标目录外面的路径。
        seen += item.compressed_size();
        if seen - last > 4 << 20 {
            last = seen;
            ui::progress_labeled("解包", seen, total);
        }
        let Some(rel) = item.enclosed_name() else {
            continue;
        };
        let out = into.join(&rel);

        if item.is_dir() {
            crate::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            crate::fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        if item.is_symlink() {
            let mut target = String::new();
            item.read_to_string(&mut target)
                .context(Code::Io, "读符号链接", "安装包可能损坏")?;
            let _ = std::fs::remove_file(&out);
            std::os::unix::fs::symlink(&target, &out).context(
                Code::Io,
                format!("建符号链接 {}", out.display()),
                "确认 ~/.vkx 可写",
            )?;
            continue;
        }

        let mut sink = std::fs::File::create(&out).context(
            Code::Io,
            format!("写 {}", out.display()),
            "确认 ~/.vkx 可写、磁盘还有空间",
        )?;
        std::io::copy(&mut item, &mut sink).context(Code::Io, "解压", "确认磁盘还有空间")?;

        // zip 的 unix 权限在扩展属性里。没有就按普通文件处理——上游的 zip
        // 基本都带，带不了的（Windows 打的包）那边也不看这一位。
        #[cfg(unix)]
        if let Some(mode) = item.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
        }
    }
    ui::progress_done();
    Ok(())
}

/// 解开之后，真正要的东西在哪一层。
///
/// 上游包的外壳各不相同：NDK 是一层同名目录、PyPI 轮子把二进制放在
/// `<包名>/data/bin`、macOS 的 JDK 在 `Contents/Home`。`pick` 给了就按它找，
/// 没给就自动剥掉单层外壳。
fn locate(root: &Path, pick: &str) -> Result<PathBuf> {
    if !pick.is_empty() {
        let direct = root.join(pick);
        if direct.is_dir() {
            return Ok(direct);
        }
        // 上游有时在外面还套一层，往下找两层。
        for depth in 1..=2 {
            if let Some(found) = search(root, pick, depth) {
                return Ok(found);
            }
        }
        return Err(Error::new(
            Code::MissingComponent,
            format!("包里找不到 {pick}"),
            "上游的包结构变了，这一版 vkx 对不上",
        ));
    }

    let mut cur = root.to_path_buf();
    loop {
        let mut items = match std::fs::read_dir(&cur) {
            Ok(d) => d.filter_map(|e| e.ok()).collect::<Vec<_>>(),
            Err(_) => break,
        };
        // macOS 解压常常留下 __MACOSX 之类的杂物，剥壳时不算数。
        items.retain(|e| !e.file_name().to_string_lossy().starts_with("__"));
        if items.len() == 1 && items[0].path().is_dir() {
            cur = items[0].path();
        } else {
            break;
        }
    }
    Ok(cur)
}

fn search(root: &Path, pick: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let candidate = p.join(pick);
        if candidate.is_dir() {
            return Some(candidate);
        }
        if depth > 1 {
            if let Some(found) = search(&p, pick, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// `vkx install <安装包>`
///
/// 只补缺的：已经装好并且戳对得上的直接跳过。`force` 则不管戳，全部重装。
pub fn install_from(bundle: &Path, force: bool) -> Result<()> {
    let host = host()?;
    let file = std::fs::File::open(bundle).context(
        Code::Io,
        format!("打开 {}", bundle.display()),
        "确认路径没写错、文件下全了",
    )?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| {
        Error::new(
            Code::Usage,
            format!("这不像一个 zip：{e}"),
            "要的是 vkx-<平台>.zip 那种离线安装包",
        )
    })?;

    let all: Vec<&Entry> = sdk::entries(host).collect();
    let todo: Vec<&Entry> = all
        .iter()
        .copied()
        .filter(|e| force || !installed(e))
        .collect();

    ui::step(&format!(
        "{}：{} 个组件，{} 个要装",
        host.name(),
        all.len(),
        todo.len()
    ));
    if todo.is_empty() {
        ui::info("都齐了，没事可做。");
        return Ok(());
    }

    let work = sdk_dir().join(".work");
    let _ = std::fs::remove_dir_all(&work);
    crate::fs::create_dir_all(&work)?;

    let mut done = 0usize;
    let mut failed: Vec<(&str, String)> = Vec::new();

    for (idx, entry) in todo.iter().enumerate() {
        // 安装包里的成员名是 <平台>/deps/<文件名>。
        let member = format!("{}/deps/{}", host.name(), entry.file);
        let mut item = match zip.by_name(&member) {
            Ok(f) => f,
            Err(_) => {
                failed.push((entry.name, format!("安装包里没有 {member}")));
                continue;
            }
        };

        ui::step(&format!(
            "[{}/{}] {} —— {}",
            idx + 1,
            todo.len(),
            entry.name,
            entry.about
        ));

        // 先落到临时文件，边写边算哈希——取出和校验一趟做完，不用把文件读两遍。
        let staged = work.join(entry.file);
        let expect = item.size();
        let got = {
            let mut sink = std::fs::File::create(&staged).context(
                Code::Io,
                format!("写 {}", staged.display()),
                "确认 ~/.vkx 可写、磁盘还有空间",
            )?;
            let mut hasher = blake3::Hasher::new();
            let mut buf = vec![0u8; 1 << 20];
            let mut copied = 0u64;
            let mut last = 0u64;
            loop {
                let n = item
                    .read(&mut buf)
                    .context(Code::Io, "从安装包里取出", "安装包可能损坏")?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                std::io::Write::write_all(&mut sink, &buf[..n])
                    .context(Code::Io, "写入", "确认磁盘还有空间")?;
                copied += n as u64;
                if copied - last > 4 << 20 {
                    last = copied;
                    ui::progress_labeled("取出", copied, expect);
                }
            }
            ui::progress_done();
            hasher.finalize().to_hex().to_string()
        };
        if got != entry.blake3 {
            let _ = std::fs::remove_file(&staged);
            failed.push((
                entry.name,
                format!("blake3 对不上\n    要 {}\n    得 {got}", entry.blake3),
            ));
            continue;
        }

        // 解到一个临时目录，剥完壳再整个搬过去。中途失败不会留下装了一半的东西。
        let raw = work.join(format!("{}.raw", entry.name));
        let _ = std::fs::remove_dir_all(&raw);
        if let Err(e) = unpack(&staged, &raw, entry.format) {
            let _ = std::fs::remove_dir_all(&raw);
            let _ = std::fs::remove_file(&staged);
            failed.push((entry.name, format!("解包失败：{}", e.message)));
            continue;
        }
        let _ = std::fs::remove_file(&staged);

        let from = match locate(&raw, entry.pick) {
            Ok(p) => p,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&raw);
                failed.push((entry.name, e.message.clone()));
                continue;
            }
        };

        let dest = sdk_dir().join(entry.dest);
        let _ = std::fs::remove_dir_all(&dest);
        if let Some(parent) = dest.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        if std::fs::rename(&from, &dest).is_err() {
            // 跨文件系统时 rename 会失败，退回逐个复制。
            copy_tree(&from, &dest)?;
        }
        let _ = std::fs::remove_dir_all(&raw);

        crate::fs::create_dir_all(&sdk_dir().join(".installed"))?;
        crate::fs::write(&stamp_path(entry), entry.blake3)?;
        done += 1;
    }

    let _ = std::fs::remove_dir_all(&work);

    if failed.is_empty() {
        ui::step(&format!("装好 {done} 个"));
        return Ok(());
    }
    let list = failed
        .iter()
        .map(|(n, why)| format!("  {n}：{why}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(Error::new(
        Code::MissingComponent,
        format!("装好 {done} 个，{} 个没装上：\n{list}", failed.len()),
        "校验不过的已经删掉了，没留半成品。换一个安装包重试",
    ))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    crate::fs::create_dir_all(to)?;
    for item in std::fs::read_dir(from).context(Code::Io, "读目录", "确认权限")? {
        let item = item.context(Code::Io, "读目录项", "确认权限")?;
        let src = item.path();
        let dst = to.join(item.file_name());
        let kind = item.file_type().context(Code::Io, "看类型", "确认权限")?;
        if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            {
                let target = std::fs::read_link(&src).context(Code::Io, "读链接", "确认权限")?;
                let _ = std::os::unix::fs::symlink(target, &dst);
            }
        } else {
            std::fs::copy(&src, &dst).context(Code::Io, "复制", "确认磁盘还有空间")?;
        }
    }
    Ok(())
}

/// `vkx doctor` 里那张表。
pub fn status() -> Result<()> {
    let host = host()?;
    let entries: Vec<&Entry> = sdk::entries(host).collect();
    let have = entries.iter().filter(|e| installed(e)).count();

    ui::step(&format!(
        "开发平台 {}，{}/{} 个组件已安装",
        host.name(),
        have,
        entries.len()
    ));
    for e in &entries {
        let mark = if installed(e) { "已装" } else { "缺" };
        println!("  {mark:<4} {:<24} {}", e.name, e.about);
    }
    if have < entries.len() {
        ui::info(&format!(
            "补齐：vkx install vkx-{}.zip",
            host.name()
        ));
    }
    Ok(())
}

// ===========================================================================
// 各命令要哪些组件
// ===========================================================================
// 写在这里而不是散在各处，是因为「缺什么」这个问题的答案必须和 `vkx doctor`
// 是同一份——两处各写一份，迟早对不上。

/// 桌面构建：编译、着色器、链接、跑起来。
const DESKTOP: &[&str] = &["cmake", "ninja", "slang", "vulkan-sdk", "libs"];
/// macOS 上还要 MoltenVK 才有 Vulkan。
const MACOS_EXTRA: &[&str] = &["moltenvk"];
/// 出安卓包那一整套。
const ANDROID: &[&str] = &[
    "jdk",
    "gradle",
    "android-ndk",
    "android-cmdline-tools",
    "android-build-tools",
    "android-platform",
    "android-platform-tools",
];

fn desktop_set(host: Host) -> Vec<&'static str> {
    let mut v = DESKTOP.to_vec();
    if host == Host::MacosArm64 {
        v.extend_from_slice(MACOS_EXTRA);
    }
    v
}

/// 桌面构建之前查一遍。
pub fn require_desktop() -> Result<()> {
    let host = host()?;
    require(&desktop_set(host), "桌面构建")
}

/// 安卓构建之前查一遍：桌面那套也要，安卓是加在它上面的。
pub fn require_android() -> Result<()> {
    let host = host()?;
    let mut need = desktop_set(host);
    need.extend_from_slice(ANDROID);
    require(&need, "安卓构建")
}

/// iOS 构建之前查一遍。Xcode 不在这里管——那个 vkx 装不了，`vkx doctor` 单独报。
pub fn require_ios() -> Result<()> {
    let host = host()?;
    require(&desktop_set(host), "iOS 构建")
}

/// `vkx fmt` 之前查一遍。
pub fn require_fmt() -> Result<()> {
    require(&["clang-format"], "代码格式化")
}

// ===========================================================================
// 卸载
// ===========================================================================

/// 递归量一个目录多大，只是为了在确认提示里说清要删多少东西。
fn dir_size(path: &Path) -> u64 {
    let Ok(items) = std::fs::read_dir(path) else {
        return 0;
    };
    items
        .filter_map(|e| e.ok())
        .map(|e| match e.file_type() {
            // 符号链接不跟进去，否则 NDK 里那几百个链接会被重复计。
            Ok(t) if t.is_symlink() => 0,
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

/// `vkx self uninstall`
///
/// vkx 装的东西全在 `~/.vkx` 一个目录里，卸载就是删掉它。没有注册表、没有
/// 散落在 /usr/local 的软链、没有改过的系统配置——这是当初把一切都塞进
/// `~/.vkx` 的理由之一。
pub fn uninstall(yes: bool) -> Result<()> {
    let home = vkx_home();
    if !home.exists() {
        ui::info(&format!("{} 不存在，没什么可卸的。", home.display()));
        return Ok(());
    }

    let size = dir_size(&home);
    ui::step(&format!(
        "要删掉 {}（约 {:.1} GB）",
        home.display(),
        size as f64 / 1_073_741_824.0
    ));

    if !yes {
        if !crate::prompt::interactive() {
            return Err(Error::new(
                Code::Usage,
                "当前不是交互式终端，不会不问就删",
                "确认要删就加上 --yes：vkx self uninstall --yes",
            ));
        }
        let answer = crate::prompt::ask("确认删除？输入 yes", Some("no"), |_| Ok(()))?;
        if answer.trim() != "yes" {
            ui::info("取消了，什么都没动。");
            return Ok(());
        }
    }

    // Windows 上删不掉正在运行的 .exe。如果 vkx 自己就在要删的目录里，
    // 先把别的都删掉，再告诉用户那一个文件怎么处理——比整个失败要好。
    let running = std::env::current_exe().ok();
    let self_inside = running
        .as_ref()
        .map(|p| p.starts_with(&home))
        .unwrap_or(false);

    if cfg!(windows) && self_inside {
        let keep = running.clone().unwrap_or_default();
        remove_except(&home, &keep)?;
        ui::step("除了 vkx 自己，其余都删掉了");
        ui::info(&format!(
            "最后一步得你来（Windows 不让删正在运行的程序）：\n    del \"{}\"",
            keep.display()
        ));
        return Ok(());
    }

    crate::fs::remove_dir_all(&home)?;
    ui::step("卸载完成");
    if self_inside {
        // Unix 上删掉正在跑的二进制是合法的，inode 会活到进程退出。
        ui::info("vkx 自己也一起删了。如果之前把它加进过 PATH，记得把那行去掉。");
    }
    Ok(())
}

/// 删掉目录下除 `keep` 以外的一切。
fn remove_except(dir: &Path, keep: &Path) -> Result<()> {
    let items = std::fs::read_dir(dir).context(Code::Io, "读目录", "确认权限")?;
    for item in items.filter_map(|e| e.ok()) {
        let path = item.path();
        if keep.starts_with(&path) {
            if path.is_dir() {
                remove_except(&path, keep)?;
            }
            continue;
        }
        let _ = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
    }
    Ok(())
}
