//! vkx 的离线安装器。
//!
//! # 它是什么
//!
//! 这个可执行文件后面直接拼着一个 zip（`cat setup bundle.zip > vkx-setup`）。
//! zip 的中央目录在文件末尾，读 zip 的程序都是从尾巴往回找的，所以前面拼多少
//! 字节都不影响——一个可执行文件加一个 zip，仍然是一个合法的 zip。自解压包
//! 几十年来就是这么做的。
//!
//! # 于是不用把 1.4 GB 解到磁盘上
//!
//! 既然自己就是合法 zip，就不必先把 bundle 解出来再喂给 vkx——把自己的路径
//! 交给 `vkx install` 就行。省掉一次 1.4 GB 的临时拷贝，也就没有「装完还要
//! 记得清理临时文件」这回事。
//!
//! 真正要解出来的只有一个文件：vkx 自己。

#[path = "../payload.rs"]
mod payload;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 这个安装器是给哪个平台编的。zip 里的目录名用的就是这套。
const HOST: &str = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    "windows-x64"
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    "linux-x64"
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "macos-arm64"
} else {
    "unsupported"
};

const EXE: &str = if cfg!(windows) { "vkx.exe" } else { "vkx" };
/// 让 PATH 在**当前这个** shell 里立刻生效的那一句。
///
/// 进程改不了父 shell 的环境变量——这是操作系统的规矩，装完就自动生效做不到。
/// 能做的是把这一句现成地摆出来，省得读者去翻文档，或者被迫重开一个终端。
const ACTIVATE: &str = if cfg!(windows) {
    r#"$env:Path += ";$env:USERPROFILE\.vkx\bin""#
} else {
    r#"export PATH="$HOME/.vkx/bin:$PATH""#
};

fn die(what: &str) -> ! {
    eprintln!();
    eprintln!("装不下去了：{what}");
    eprintln!();
    wait_if_double_clicked();
    std::process::exit(1);
}

/// 双击运行时窗口会在进程退出的一瞬间关掉，什么都来不及看。
/// 只在「标准输入是终端」时不等——那说明是在命令行里跑的。
fn wait_if_double_clicked() {
    use std::io::IsTerminal;
    if cfg!(windows) && !std::io::stdin().is_terminal() {
        return;
    }
    if cfg!(windows) {
        eprintln!("按回车关闭…");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}

fn home() -> PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    match std::env::var_os(key) {
        Some(v) => PathBuf::from(v),
        None => die("找不到用户主目录"),
    }
}

fn main() {
    if HOST == "unsupported" {
        die("vkx 只支持 windows-x64 / linux-x64 / macos-arm64 三个开发平台");
    }

    println!("vkx 离线安装 —— {HOST}");
    println!();

    let me = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => die(&format!("定位不到安装器自己：{e}")),
    };

    // 1. 从自己身上取出 vkx
    let vkx_home = std::env::var_os("VKX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".vkx"));
    let bin_dir = vkx_home.join("bin");
    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        die(&format!("建不了 {}：{e}", bin_dir.display()));
    }
    let vkx = bin_dir.join(EXE);
    extract_self(&me, &format!("{HOST}/{EXE}"), &vkx);
    println!("  取出 {}", vkx.display());

    // 2. 让 vkx 装它自己要的一切。安装器把自身路径交过去——它就是那个 zip。
    println!();
    run(&vkx, &["install", &me.to_string_lossy()]);

    // 3. 验一遍
    println!();
    run(&vkx, &["doctor"]);

    println!();
    println!("装好了。当前这个终端里先跑一句，让 PATH 立刻生效：");
    println!();
    println!("    {ACTIVATE}");
    println!();
    println!("然后：");
    println!();
    println!("    vkx new client");
    println!("    cd client");
    println!("    vkx run");
    println!();
    println!("（PATH 已经写进配置了，以后新开的终端不用再跑那一句。）");
    println!();
    println!("要卸载：vkx self uninstall");
    wait_if_double_clicked();
}

/// 把自己这个 zip 里的某一项解到指定位置。
fn extract_self(me: &Path, member: &str, to: &Path) {
    let mut zip = match payload::open(me) {
        Ok(z) => z,
        Err(e) => die(&format!(
            "这个安装器里没有可读的数据（{e}）。\n  \
             多半是下载没下全，或者被杀毒软件改过。重新下一份。"
        )),
    };
    let mut item = match zip.by_name(member) {
        Ok(f) => f,
        Err(_) => die(&format!("安装包里没有 {member}")),
    };
    let mut buf = Vec::new();
    if let Err(e) = item.read_to_end(&mut buf) {
        die(&format!("读 {member} 失败：{e}"));
    }
    // 先写临时名再改名：万一写到一半断电，也不会留下一个半截的 vkx。
    let tmp = to.with_extension("partial");
    if let Err(e) = std::fs::write(&tmp, &buf) {
        die(&format!("写 {}：{e}", tmp.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_file(to);
    if let Err(e) = std::fs::rename(&tmp, to) {
        die(&format!("放置 {}：{e}", to.display()));
    }
}

fn run(vkx: &Path, args: &[&str]) {
    let status = Command::new(vkx).args(args).status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => die(&format!("vkx {} 失败（退出码 {}）", args[0], s.code().unwrap_or(-1))),
        Err(e) => die(&format!("跑不了 vkx：{e}")),
    }
}
