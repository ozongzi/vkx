//! vkx 的错误。
//!
//! 一条错误必须回答三个问题：出了什么事、为什么、现在该做什么。第三个是硬性
//! 要求——[`Error::new`] 的第二个参数就是解法，构造不出没有解法的错误。
//!
//! 长篇说明放在 `vkx help E0012` 里，短消息保持一行。

use std::fmt;

/// 错误码。短消息里带上它，读者可以直接搜，也可以 `vkx help E0012` 看长篇。
///
/// 只增不改：一个码一旦发布就不再改变含义，否则搜出来的旧帖子会指向别的东西。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// 不在 vkx 工程里
    NotAProject,
    /// 工程文件读不了或者写错了
    BadManifest,
    /// 需要的工具链组件不在 ~/.vkx 里
    MissingComponent,
    /// 外部命令跑失败了
    CommandFailed,
    /// 文件系统操作失败
    Io,
    /// 平台或环境不满足要求（缺 Xcode、不是 macOS、没连设备……）
    Environment,
    /// 用法不对（参数缺失、非交互环境下要求输入……）
    Usage,
}

impl Code {
    /// `E` 加四位，稳定不变。
    pub fn id(self) -> &'static str {
        match self {
            Code::NotAProject => "E0001",
            Code::BadManifest => "E0002",
            Code::MissingComponent => "E0003",
            Code::CommandFailed => "E0004",
            Code::Io => "E0005",
            Code::Environment => "E0006",
            Code::Usage => "E0007",
        }
    }

    /// `vkx help <码>` 打印的长篇说明。
    pub fn explain(self) -> &'static str {
        match self {
            Code::NotAProject => {
                "vkx 靠工程根目录下的 vkx.toml 认出一个工程，并从当前目录逐级往上找。\n\
                 在工程外面运行 build / run / dist 这些命令就会得到这个错误。\n\n\
                 cd 进工程目录，或者用 `vkx new <名字>` 新建一个。"
            }
            Code::BadManifest => {
                "vkx.toml 读不出来，或者缺了必填字段。\n\n\
                 必填的是 [project] 下的 name 和 package_id。全部字段见 `vkx help manifest`。"
            }
            Code::MissingComponent => {
                "工具链的某个组件不在 ~/.vkx 里。\n\n\
                 vkx 按需下载，所以第一次用到某个平台时会去取。\n\
                 网络不通时可以先 `vkx fetch` 在有网的地方备好，\n\
                 或者用 VKX_MIRROR 指向别的站点。"
            }
            Code::CommandFailed => {
                "vkx 调用的外部命令（cmake、ninja、slangc、gradle、xcodebuild 之一）\n\
                 返回了非零退出码。它自己的输出在上面。\n\n\
                 如果那段输出指向 target/CMakeLists.txt，注意那个文件是生成的——\n\
                 要改的是 vkx.toml。"
            }
            Code::Io => {
                "读写文件失败。常见原因是权限不足、路径不存在、或者磁盘满了。\n\n\
                 `vkx clean --cache` 可以清掉下载缓存腾出空间。"
            }
            Code::Environment => {
                "当前环境不满足这条命令的要求。有两样东西 vkx 无法代为安装：\n\n\
                 - macOS 的 Xcode 和命令行工具（Apple 不允许第三方再分发）\n\
                 - 显卡驱动里的 Vulkan ICD（由驱动提供）\n\n\
                 其余组件都可以 `vkx fetch` 取回来。"
            }
            Code::Usage => {
                "命令的用法不对。`vkx help <命令>` 有该命令的完整说明。\n\n\
                 非交互环境（管道、CI）下 vkx 不会停下来等输入，\n\
                 需要输入的参数必须在命令行上写全。"
            }
        }
    }
}

/// 一条错误：出了什么事 + 该怎么办。
pub struct Error {
    pub code: Code,
    pub message: String,
    /// 至少一条。第一条由 [`Error::new`] 强制要求，后续用 [`Error::hint`] 追加。
    pub hints: Vec<String>,
}

impl Error {
    /// `message` 说出了什么事，`fix` 说现在该做什么。两个都必填。
    pub fn new(code: Code, message: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints: vec![fix.into()],
        }
    }

    /// 追加一条解法。
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 给 `Result` 和 `Option` 加上下文。
///
/// 故意不提供 `From<std::io::Error>`：自动转换会产生没有解法的裸错误，而且
/// 一旦有了它，漏加上下文的地方编译器就不再提醒。所有可能失败的调用都必须
/// 显式说清「在做什么」和「怎么办」。
pub trait Context<T> {
    fn context(self, code: Code, what: impl Into<String>, fix: impl Into<String>) -> Result<T>;
}

impl<T, E: fmt::Display> Context<T> for std::result::Result<T, E> {
    fn context(self, code: Code, what: impl Into<String>, fix: impl Into<String>) -> Result<T> {
        self.map_err(|e| Error::new(code, format!("{}: {e}", what.into()), fix))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, code: Code, what: impl Into<String>, fix: impl Into<String>) -> Result<T> {
        self.ok_or_else(|| Error::new(code, what.into(), fix))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
