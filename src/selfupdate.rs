//! `vkx self update` —— 从镜像换掉自己这个二进制。
//!
//! Unix 上直接 rename 覆盖就行：正在运行的进程持有旧 inode，不受影响。
//! Windows 不允许覆盖正在运行的 exe，所以先把自己改名让出位置，下次启动再清掉。

use crate::error::{Code, Context, Error, Result};
use crate::{fetch, ui};

/// 启动时顺手清掉上次更新留下的旧文件（只有 Windows 会留）。
pub fn sweep_old() {
    if let Ok(exe) = std::env::current_exe() {
        let old = exe.with_extension("old.exe");
        if old.exists() {
            let _ = std::fs::remove_file(old);
        }
    }
}

fn remote_version() -> Result<String> {
    let url = format!("{}/vkx/version.txt", fetch::mirror());
    let text = ureq::get(&url)
        .call()
        .and_then(|r| r.into_body().read_to_string())
        .map_err(|e| {
            Error::new(
                Code::MissingComponent,
                format!(
                    "取不到版本信息：{url}
  {e}"
                ),
                "确认网络可达，或换一个站点：VKX_MIRROR=<地址> vkx self update",
            )
        })?;
    Ok(text.trim().to_string())
}

pub fn run(check_only: bool) -> Result<u8> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = remote_version()?;

    if latest == current {
        ui::step(&format!("已经是最新版 {current}"));
        return Ok(0);
    }
    ui::step(&format!("有新版本：{current} → {latest}"));
    if check_only {
        ui::info("运行 `vkx self update` 更新。");
        return Ok(0);
    }

    let platform = fetch::platform();
    let url = format!("{}/vkx/{latest}/vkx-{latest}-{platform}", fetch::mirror());
    let staged = std::env::temp_dir().join(format!("vkx-{latest}"));
    fetch::download_to(&url, &staged)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).context(
            Code::Io,
            "给新版本加执行权限",
            "确认临时目录可写",
        )?;
    }

    let exe = std::env::current_exe().context(
        Code::Io,
        "定位当前可执行文件",
        "确认 vkx 是从文件系统启动的",
    )?;

    // Windows 不能覆盖正在跑的 exe：先把自己让开，下次启动时 sweep_old 清掉。
    if cfg!(windows) {
        let old = exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).context(
            Code::Io,
            "把旧版本让开",
            "确认 ~/.vkx/bin 可写，且没有别的 vkx 正在运行",
        )?;
    }

    std::fs::rename(&staged, &exe).or_else(|_| {
        // 跨文件系统时 rename 会失败，退回复制。
        crate::fs::copy(&staged, &exe)?;
        crate::fs::remove_file(&staged)
    })?;

    ui::step(&format!("已更新到 {latest}"));
    ui::info("依赖集跟着 vkx 版本走，跑一次 `vkx fetch` 把对应的组件取回来。");
    Ok(0)
}
