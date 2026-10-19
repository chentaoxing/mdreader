//! 核心错误类型。跨平台可移植：只使用 core/std 语言特性，不使用 io::Error 具体形态。

use std::fmt;

pub type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// 输入不是合法 UTF-8。核心引擎只接受 UTF-8 字节序列（平台层负责解码，
    /// 解码失败必须报错，禁止猜测编码后静默转换）。
    InvalidUtf8,
    /// 文档过大，超出当前配置的内存处理上限。
    TooLarge { limit: usize, actual: usize },
    /// 规则产生了重叠 / 非法的编辑区间，属于内部缺陷。
    OverlappingEdits { rule: String },
    /// 规则输出未通过 Content Guard，已丢弃（不算致命错误，会体现在报告中）。
    GuardRejected { rule: String, detail: String },
    /// 解析层失败。
    Parse(String),
    /// 请求的规则 id 不存在。
    UnknownRule(String),
    /// 处理被调用方取消（大文档异步 UI 使用）。
    Cancelled,
    /// 平台文件抽象层失败，错误文案由各平台适配器提供。
    Storage(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::InvalidUtf8 => write!(f, "input is not valid UTF-8"),
            CoreError::TooLarge { limit, actual } => {
                write!(f, "document too large: {actual} bytes > limit {limit}")
            }
            CoreError::OverlappingEdits { rule } => {
                write!(f, "rule `{rule}` produced overlapping edits")
            }
            CoreError::GuardRejected { rule, detail } => {
                write!(f, "rule `{rule}` rejected by content guard: {detail}")
            }
            CoreError::Parse(msg) => write!(f, "parse error: {msg}"),
            CoreError::UnknownRule(id) => write!(f, "unknown rule id: {id}"),
            CoreError::Cancelled => write!(f, "operation cancelled"),
            CoreError::Storage(msg) => write!(f, "storage error: {msg}"),
        }
    }
}

impl std::error::Error for CoreError {}
