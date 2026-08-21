//! 自解压包里的数据从哪儿开始。
//!
//! # 为什么不能让 zip 库自己找
//!
//! 自解压包是「可执行文件 + zip」直接拼起来的。zip 的中央目录在末尾，读的时候
//! 从尾巴往回扫签名，所以前面拼多少字节通常都不影响——这是自解压包几十年的做法。
//!
//! 但我们的 zip 是**存储不压缩**的（里面装的本来就是压缩包，再压一遍只费时间），
//! 于是 NDK 那个 .zip 的字节原样躺在里面，**连它自己的中央目录和 EOCD 一起**。
//! 往回扫的时候，库先找到外层 EOCD，再去算「整个压缩包相对文件头偏移了多少」，
//! 这一步会撞上内层那份签名，算出来的偏移落进 NDK 的数据里，最后解析出来的是
//! NDK 的目录树——8897 个条目，一个都不是我们要的。
//!
//! 更糟的是它不一定失败：偏移碰巧对上时能正常工作，换个组件大小就崩。这种
//! 「有时对」的东西不能留。
//!
//! # 所以自己记
//!
//! 拼的时候在最后追加 16 个字节：
//!
//! ```text
//! [可执行文件][zip][8 字节小端偏移][8 字节魔数 VKXPAY01]
//! ```
//!
//! 读的时候先看末尾 16 字节。有魔数就按记下来的偏移开一个只覆盖 zip 那一段的
//! 视图；没有就当普通 zip 从头读。两种情况返回同一个类型，上层不用分情况。

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 8] = b"VKXPAY01";
const TRAILER: u64 = 16;

/// 文件里的一段，当成独立文件来读。
///
/// zip 库要 `Read + Seek`，而它算的偏移是相对压缩包开头的；给它一个从真正
/// 开头起算的视图，它就不必知道前面还拼着别的东西。
pub struct Slice<R> {
    inner: R,
    start: u64,
    len: u64,
    pos: u64,
}

impl<R: Read + Seek> Read for Slice<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len {
            return Ok(0);
        }
        let room = (self.len - self.pos).min(buf.len() as u64) as usize;
        self.inner.seek(SeekFrom::Start(self.start + self.pos))?;
        let n = self.inner.read(&mut buf[..room])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for Slice<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let want = match from {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.len as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if want < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "定位到了负数位置",
            ));
        }
        self.pos = want as u64;
        Ok(self.pos)
    }
}

/// 打开一个安装包：自解压的按记下的偏移开，普通 zip 从头开。
pub fn open(path: &Path) -> io::Result<zip::ZipArchive<Slice<File>>> {
    let mut file = File::open(path)?;
    let total = file.seek(SeekFrom::End(0))?;

    let mut start = 0u64;
    let mut len = total;
    if total > TRAILER {
        file.seek(SeekFrom::Start(total - TRAILER))?;
        let mut tail = [0u8; TRAILER as usize];
        file.read_exact(&mut tail)?;
        if &tail[8..] == MAGIC {
            start = u64::from_le_bytes(tail[..8].try_into().unwrap());
            len = total - TRAILER - start;
        }
    }

    zip::ZipArchive::new(Slice {
        inner: file,
        start,
        len,
        pos: 0,
    })
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// 拼一个自解压包：可执行文件 + zip + 记着偏移的尾巴。
pub fn concat(stub: &Path, zip_file: &Path, out: &Path) -> io::Result<()> {
    let mut sink = File::create(out)?;
    let offset = io::copy(&mut File::open(stub)?, &mut sink)?;
    io::copy(&mut File::open(zip_file)?, &mut sink)?;
    use std::io::Write;
    sink.write_all(&offset.to_le_bytes())?;
    sink.write_all(MAGIC)?;
    Ok(())
}
