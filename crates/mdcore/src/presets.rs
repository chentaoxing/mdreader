//! 风格预设（规格第二十六章）。
//!
//! 预设只调**参数与开关**，不调内容策略：任何预设下 Content Guard 的判定标准完全相同，
//! 「正文不变」不是可调项。预设名是稳定字符串，写入工程清单以便跨设备复现同一套参数。

use crate::rules::{Params, Profile, Rule};

/// 内置预设标识。
pub const GENERIC: &str = "通用文章";
pub const TECHNICAL: &str = "技术文档";
pub const AI_GENERATED: &str = "AI 生成文档";
pub const FICTION: &str = "小说";

pub const PRESETS: &[&str] = &[GENERIC, TECHNICAL, AI_GENERATED, FICTION];

/// 按预设名构造运行配置。
///
/// 名字无效时返回 `None`（而不是静默回落到默认预设），UI 可据此明确提示用户。
pub fn profile_for(name: &str) -> Option<Profile> {
    let matched = PRESETS.iter().find(|p| **p == name)?;
    let params = match *matched {
        TECHNICAL => Params {
            // 技术文档：标题层级更敏感，段落允许更宽，保留列表并行分行
            max_blank_lines: 1,
            long_paragraph_width: 200,
            min_lines_under_heading: 1,
            enable_parallel_split: true,
        },
        AI_GENERATED => Params {
            // AI 生成文档：多余空行与并列堆砌最常见，收紧空白，启用分行
            max_blank_lines: 1,
            long_paragraph_width: 160,
            min_lines_under_heading: 2,
            enable_parallel_split: true,
        },
        FICTION => Params {
            // 小说：段落即节奏，不做并列分行，空行允许更多（场景分隔）
            max_blank_lines: 2,
            long_paragraph_width: 240,
            min_lines_under_heading: 0,
            enable_parallel_split: false,
        },
        _ => Params::default(),
    };
    let mut profile = Profile { params, ..Profile::default() };
    match *matched {
        FICTION => {
            // 小说不统一列表符号（对话/歌谣里的符号是排版的一部分）
            profile.disabled.push(crate::rules::ListMarkerUnify.id().to_string());
        }
        TECHNICAL => {
            // 技术文档不做 setext -> atx（部分仓库依赖 setext 风格做 diff）
            profile.disabled.push(crate::rules::SetextToAtx.id().to_string());
            profile.max_passes = 2;
        }
        _ => {}
    }
    Some(profile)
}

/// 预设说明，供设置页展示；不承诺任何内容层面的改写。
pub fn describe(name: &str) -> &'static str {
    match name {
        GENERIC => "默认：结构规范优先，改动最小",
        TECHNICAL => "技术文档：宽松段落宽度、保留 setext 风格、两轮收敛",
        AI_GENERATED => "AI 生成文档：收紧空白与标题下空壳，启用并列项分行",
        FICTION => "小说：保留排版空行，不改列表符号，不做并列分行",
        _ => "未知预设",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Optimizer;
    use crate::rules::Registry;

    #[test]
    fn all_presets_are_resolvable() {
        for p in PRESETS {
            assert!(profile_for(p).is_some(), "{p}");
            assert!(!describe(p).contains("未知"));
        }
        assert!(profile_for("不存在的预设").is_none());
    }

    #[test]
    fn every_preset_keeps_content_identical() {
        let src = "# 标题\n\n\n\n这个方案有三个问题：成本高、部署复杂、维护困难。\n\n\n\n正文\n\n- b\n- a\n";
        let registry = Registry::new();
        for p in PRESETS {
            let profile = profile_for(p).unwrap();
            let o = Optimizer::new(&registry, &profile).run(src).unwrap();
            assert_eq!(o.content_changes(), 0, "{p} 预设下正文发生变化");
            assert_eq!(o.original_body, src);
        }
    }

    #[test]
    fn fiction_preset_does_not_split_parallel() {
        let src = "这个方案有三个问题：成本高、部署复杂、维护困难。\n";
        let registry = Registry::new();
        let profile = profile_for(FICTION).unwrap();
        let o = Optimizer::new(&registry, &profile).run(src).unwrap();
        assert_eq!(o.body, src, "小说预设不应分行");
    }

    #[test]
    fn generic_preset_does_split_parallel() {
        let src = "这个方案有三个问题：成本高、部署复杂、维护困难。\n";
        let registry = Registry::new();
        let o = Optimizer::new(&registry, &profile_for(GENERIC).unwrap()).run(src).unwrap();
        assert_ne!(o.body, src);
        assert_eq!(o.body.replace('\n', ""), src.replace('\n', ""), "只允许插入换行");
    }
}
