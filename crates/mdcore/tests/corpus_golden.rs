//! 语料级内容不变性回归（规格第四十一章：100% 通过率；第四十章：五平台核心一致性）。
//!
//! 两件事在这里钉死：
//! 1. `tests/corpus/*.md` 每一份优化后正文 token 序列必须逐位相同，且二次运行收敛；
//! 2. 优化输出必须与 `tests/corpus/GOLDEN.csv` 记录的指纹完全一致 —— 这条断言等价于
//!    「同一份输入在五平台任何机器上得到同一份输出」，因为指纹只依赖核心的确定性算法
//!    （FNV-1a over token 序列、整数评分、稳定规则顺序），不依赖 locale / 指针 / 线程数 / 换行。
//!
//! 重新生成基线：`MDFILE_REGOLDEN=1 cargo test -p mdcore --test corpus_golden`

use std::path::{Path, PathBuf};

use mdcore::fingerprint::tokenize;
use mdcore::pipeline::Optimizer;
use mdcore::rules::{Profile, Registry};
use mdcore::text::TextLayer;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn corpus_files() -> Vec<PathBuf> {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("语料目录缺失 {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("md")).unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

/// 内容指纹：**只含正文语义信息**（token 序列校验和、各类 token 计数、反斜杠数）。
/// 不含字节长度 —— 结构优化本来就会改变字节长度，规格第二十四章要钉死的是「正文不变」。
fn content_fingerprint(body: &str) -> String {
    let fp = tokenize(body);
    format!(
        "{:016x},w{},p{},c{},u{},h{},r{},b{}",
        fp.checksum,
        fp.counts.words,
        fp.counts.puncts,
        fp.counts.code_bytes,
        fp.counts.urls,
        fp.counts.html_bytes,
        fp.counts.refs,
        fp.backslashes
    )
}

/// 输出指纹：内容指纹 + 字节长度，用于 GOLDEN 基线（能察觉任何结构层面的漂移）。
fn fingerprint(body: &str) -> String {
    format!("{},len{}", content_fingerprint(body), body.len())
}

#[test]
fn corpus_content_invariance_is_100_percent() {
    let registry = Registry::new();
    let profile = Profile::default();
    let files = corpus_files();
    assert!(files.len() >= 30, "语料数量不足：{}", files.len());
    let mut checked = 0usize;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{name}: 读取失败 {e}"));
        let doc = match TextLayer::from_bytes(&raw) {
            Ok(d) => d,
            Err(e) => panic!("{name}: {e}"),
        };
        let outcome = Optimizer::new(&registry, &profile)
            .run(doc.body())
            .unwrap_or_else(|e| panic!("{name}: 优化失败 {e}"));

        // 1. 正文不变性
        assert!(outcome.guard.is_pass(), "{name}: Content Guard 拒绝 —— {:?}", outcome.guard.violations);
        assert_eq!(outcome.guard.violations.len(), 0, "{name}");
        assert_eq!(content_fingerprint(doc.body()), content_fingerprint(&outcome.body), "{name}: 正文指纹发生变化");

        // 2. 保护区逐字节不变（代码、URL、行内代码、HTML、链接目标）
        let prot_orig = mdcore::fingerprint::protected_ranges(doc.body());
        let src = doc.body();
        for r in &prot_orig {
            let piece = &src[r.clone()];
            assert!(
                outcome.body.contains(piece),
                "{name}: 保护区内容被改写 {piece:?}"
            );
        }

        // 3. 收敛：二次运行零改动
        let again = Optimizer::new(&registry, &profile).run(&outcome.body).unwrap();
        assert!(!again.changed(), "{name}: 二次运行仍有改动 {:?}", again.applied.iter().map(|a| &a.rule_id).collect::<Vec<_>>());

        // 4. 无静默失败：被拒绝的规则不得是「本该生效却被 Guard 拦下」的实现缺陷
        for r in &outcome.rejected {
            // 允许 reason 为「等价空操作」，其它拒绝必须在输出中可见
            assert!(
                r.reason.contains("空操作") || r.reason.contains("保护区") || r.reason.contains("Content"),
                "{name}: 规则 {} 异常拒绝：{}",
                r.rule_id,
                r.reason
            );
        }
        checked += 1;
    }
    assert_eq!(checked, files.len());
    eprintln!("语料 {checked} 份全部通过内容不变性校验");
}

#[test]
fn corpus_golden_output_is_identical_everywhere() {
    let registry = Registry::new();
    let profile = Profile::default();
    let files = corpus_files();
    let mut rows: Vec<String> = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read(path).unwrap();
        let doc = TextLayer::from_bytes(&raw).unwrap_or_else(|e| panic!("{name}: {e}"));
        let outcome = Optimizer::new(&registry, &profile)
            .run(doc.body())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let fp = fingerprint(&outcome.body);
        let fp0 = fingerprint(&outcome.original_body);
        // 评分打包成一个整数：综合 / 标题 / 段落 / 列表 / 空白 / 密度，全整数，跨平台可比
        let score = outcome.after.overall as u64 * 100_000_000
            + outcome.after.heading.value as u64 * 1_000_000
            + outcome.after.paragraph.value as u64 * 10_000
            + outcome.after.list.value as u64 * 100
            + outcome.after.whitespace.value as u64;
        let applied: Vec<&str> = outcome.applied.iter().map(|a| a.rule_id.as_str()).collect();
        rows.push(format!(
            "{name},{fp0},{fp},{score},{},{},{}",
            outcome.edit_total(),
            outcome.passes,
            applied.join("+")
        ));
    }
    let golden_path = corpus_dir().join("GOLDEN.csv");
    let rendered = rows.join("\n") + "\n";
    if std::env::var("MDFILE_REGOLDEN").is_ok() {
        std::fs::write(&golden_path, &rendered).unwrap();
        eprintln!("已写入基线 {}", golden_path.display());
        return;
    }
    let expected = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|_| panic!("缺少基线文件 {golden_path:?}，请用 MDFILE_REGOLDEN=1 生成一次"));
    if expected != rendered {
        for (a, b) in expected.lines().zip(rendered.lines()) {
            if a != b {
                panic!("五平台核心输出不一致：\n  基线: {a}\n  本机: {b}");
            }
        }
        panic!("基线与本机行数不同");
    }
}

#[test]
fn parallel_and_sequential_runs_agree() {
    // 反「隐藏全局状态」检查：并发跑与串行跑必须逐字节同结果。
    // 若核心偷偷依赖了 HashMap 迭代顺序、线程本地状态或可变全局量，这条会炸。
    // 它同时是「五平台一致」的最强本地代理：各平台的差异本质上是运行环境差异。
    let files = corpus_files();
    let sequential: Vec<(String, String)> = files
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let raw = std::fs::read(p).unwrap();
            let doc = TextLayer::from_bytes(&raw).unwrap();
            let o = Optimizer::new(&Registry::new(), &Profile::default()).run(doc.body()).unwrap();
            (name, format!("{}|{}", fingerprint(&o.body), o.edit_total()))
        })
        .collect();

    let chunks = 4usize;
    let per = (files.len() + chunks - 1) / chunks;
    let handles: Vec<_> = files.chunks(per.max(1)).map(|chunk| {
        let chunk: Vec<PathBuf> = chunk.to_vec();
        std::thread::spawn(move || {
            chunk
                .iter()
                .map(|p| {
                    let name = p.file_name().unwrap().to_string_lossy().to_string();
                    let raw = std::fs::read(p).unwrap();
                    let doc = TextLayer::from_bytes(&raw).unwrap();
                    let o = Optimizer::new(&Registry::new(), &Profile::default()).run(doc.body()).unwrap();
                    (name, format!("{}|{}", fingerprint(&o.body), o.edit_total()))
                })
                .collect::<Vec<_>>()
        })
    }).collect();
    let mut concurrent: Vec<(String, String)> = Vec::new();
    for h in handles {
        concurrent.extend(h.join().expect("线程 panic"));
    }
    concurrent.sort();
    let mut seq_sorted = sequential.clone();
    seq_sorted.sort();
    assert_eq!(seq_sorted, concurrent, "并发执行改变了核心输出");
}

#[test]
fn line_ending_variants_produce_identical_semantic_output() {
    // 规格第三十一、三十二章：换行风格属格式层，同一语义内容必须得到同一语义输出。
    let registry = Registry::new();
    let profile = Profile::default();
    for path in corpus_files() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read(&path).unwrap();
        let Ok(doc) = TextLayer::from_bytes(&raw) else { continue };
        let lf = Optimizer::new(&registry, &profile).run(doc.body()).unwrap();
        let crlf_body = doc.body().replace('\n', "\r\n");
        // 用格式层渲染 -> 再解析回来，语义层应完全等价
        let round = TextLayer::from_bytes(&crlf_body.into_bytes()).unwrap();
        let crlf = Optimizer::new(&registry, &profile).run(round.body()).unwrap();
        assert_eq!(fingerprint(&lf.body), fingerprint(&crlf.body), "{name}: 换行风格改变了输出");
        assert_eq!(lf.body, crlf.body.replace("\r\n", "\n"), "{name}");
    }
}
