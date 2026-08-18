//! 文件操作的薄封装。
//!
//! 标准库的 `std::fs` 出错时只说「Permission denied」，不说是哪个文件、在干什么。
//! 这里每个函数都把路径和意图带上，于是错误信息本身就够定位问题。
//!
//! [`crate::error`] 故意不提供 `From<std::io::Error>`，所以直接用 `std::fs` 加 `?`
//! 是编译不过的——要么用这里的函数，要么显式 `.context(...)`。

use std::path::{Path, PathBuf};

use crate::error::{Code, Context, Result};

const PERM: &str = "确认路径存在、有读写权限，且磁盘还有空间（`vkx clean --cache` 可以腾）";

pub fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).context(Code::Io, format!("读取 {}", path.display()), PERM)
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    std::fs::write(path, contents).context(Code::Io, format!("写入 {}", path.display()), PERM)
}

pub fn create_dir_all(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).context(Code::Io, format!("创建目录 {}", path.display()), PERM)
}

pub fn copy(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }
    std::fs::copy(from, to)
        .context(
            Code::Io,
            format!("复制 {} 到 {}", from.display(), to.display()),
            PERM,
        )
        .map(|_| ())
}

/// 删掉整棵目录；本来就不存在时当作成功。
pub fn remove_dir_all(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context(Code::Io, format!("删除目录 {}", path.display()), PERM),
    }
}

/// 删掉一个文件；本来就不存在时当作成功。
pub fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context(Code::Io, format!("删除 {}", path.display()), PERM),
    }
}

/// 列出目录里的条目，按路径排序，输出稳定。
pub fn read_dir(path: &Path) -> Result<Vec<PathBuf>> {
    let entries =
        std::fs::read_dir(path).context(Code::Io, format!("列出目录 {}", path.display()), PERM)?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.context(Code::Io, format!("读取 {} 里的条目", path.display()), PERM)?;
        out.push(entry.path());
    }
    out.sort();
    Ok(out)
}
