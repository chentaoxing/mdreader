//! mdcore —— MD文档阅读助手的跨平台核心引擎。
//!
//! 设计约束（对应产品规格第五、六、七章）：
//! - Core First：本 crate 不依赖任何平台 UI / 系统 API（不调用 Win32 / AppKit / UIKit /
//!   Android SDK / Linux Desktop API），只做 `Markdown String -> Optimized Markdown String`。
//! - 内容不可变：所有优化必须通过 [`guard::ContentGuard`] 的 token 序列校验，
//!   校验失败的操作一律丢弃，绝不部分提交。
//! - 五平台确定性：输出不依赖 HashMap 迭代顺序、不依赖浮点、不依赖平台 locale。
//! - 完全离线：无网络、无账号、无 AI。
//!
//! 语义内容层与文件格式层的区分见 [`text`]：换行符（LF/CRLF/CR）与 BOM 属于格式层，
//! 归一化不计入内容变化；正文 token 属于语义层，一个字节都不能变。

pub mod diff;
pub mod edits;
pub mod error;
pub mod fingerprint;
pub mod guard;
pub mod io;
pub mod model;
pub mod pipeline;
pub mod presets;
pub mod readability;
pub mod rules;
pub mod scan;
pub mod text;

pub use diff::{Decision, DecisionLog, Diff};
pub use edits::ChangeScope;
pub use error::{CoreError, CoreResult};
pub use guard::{GuardReport, Verdict, Violation};
pub use io::{Manifest, OpenDocument};
pub use model::{Block, BlockKind, Document};
pub use pipeline::{Optimizer, Outcome};
pub use rules::{Profile, Registry};
pub use text::{NewlineStyle, TextLayer};

/// 引擎版本，写入报告与 FFI 以便做五平台一致性校验。
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
