/*
 * mdfile.h — MD文档阅读助手核心引擎 C ABI
 *
 * 这是五个平台唯一的功能入口（规格第十三章 Shared Core）。实际产物名由 cargo 决定，
 * lib 名为 `mdcore_ffi`，别按别的名字找：
 *   Windows : mdcore_ffi.dll + 导入库 mdcore_ffi.dll.lib(MSVC) / libmdcore_ffi.a(GNU)
 *   macOS   : libmdcore_ffi.a（打进 xcframework，Swift 用 module map import）
 *   Linux   : libmdcore_ffi.so
 *   Android : libmdcore_ffi.so（cargo-ndk 产出，Kotlin/JNI 包装本头文件）
 *   iOS     : libmdcore_ffi.a（静态链接，Swift 直接 import）
 *
 * 边界约定：
 *   - 输入是字节数组（ptr + len），本层不做任何路径 / 权限假设；(ptr=NULL, len=0) 表示空文件
 *   - 输出是 UTF-8 JSON 字符串（本层分配），必须用 mdfile_free_string 释放；
 *     调用失败时 *out_json 会被本层覆写为 NULL——若两次调用复用同一变量，
 *     上一次成功返回、尚未释放的指针会被抹掉而无法释放，请每次调用前先释放旧值
 *   - out_len 是 **JSON 文本自身的 UTF-8 字节数**，不是文件字节数；
 *     要还原文件内容请解码 rendered_latin1（latin1：第 n 个字符 -> 第 n 个字节）
 *   - 返回状态码，不返回结构体，避免各平台对齐差异造成 ABI 漂移
 *   - 核心内部 panic 会被捕获为 MDFILE_ERR_PANIC，绝不会跨 ABI 展开
 *   - 错误消息是线程本地的：与失败调用同线程读取
 *   - MDFILE_ERR_TOO_LARGE 对全部接受输入的入口（optimize/analyze/preview/rebuild）一致生效；
 *     newline 参数只认 "preserve"|"lf"|"crlf"|"cr"（NULL = preserve），其余值返回 MDFILE_ERR_UNKNOWN
 */

#ifndef MDFILE_H
#define MDFILE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* 状态码 */
#define MDFILE_OK 0             /* 成功 */
#define MDFILE_ERR_UTF8 1       /* 输入不是合法 UTF-8（绝不猜测编码） */
#define MDFILE_ERR_TOO_LARGE 2  /* 输入超过上限 */
#define MDFILE_ERR_GUARD 3      /* Content Guard 拒绝：正文发生变化，调用方必须丢弃结果 */
#define MDFILE_ERR_UNKNOWN 4    /* 未知预设 / 未知规则 ID / 非法决策 */
#define MDFILE_ERR_PANIC 5      /* 内部异常，未返回任何结果 */
#define MDFILE_ERR_ARGS (-1)    /* 空指针或长度越界 */

/* 单次调用输入上限（五平台同值） */
#define MDFILE_MAX_INPUT_BYTES (64 * 1024 * 1024)

/* 预设名（UTF-8，稳定字符串，写入工程清单以便跨设备复现） */
#define MDFILE_PRESET_GENERIC "通用文章"
#define MDFILE_PRESET_TECHNICAL "技术文档"
#define MDFILE_PRESET_AI "AI 生成文档"
#define MDFILE_PRESET_FICTION "小说"

/* 换行风格（文件格式层，不计入内容变化） */
#define MDFILE_NEWLINE_PRESERVE "preserve"
#define MDFILE_NEWLINE_LF "lf"
#define MDFILE_NEWLINE_CRLF "crlf"
#define MDFILE_NEWLINE_CR "cr"

/* 引擎版本，静态字符串，不要释放 */
const char *mdfile_engine_version(void);

/*
 * 最近一次调用的错误消息（线程本地：在工作线程调用、回主线程读取会拿到空串，
 * 取错误必须与失败调用在同一个线程）。
 * 返回值**总是**本层分配的堆内存（无错误时是长度为 0 的串），
 * 一律可以用 mdfile_free_string 释放；绝不要把返回值当静态串长期持有。
 */
char *mdfile_last_error(void);

/*
 * 释放本层返回的字符串。NULL 是合法空操作。
 * 释放后请自行把本地变量置 NULL：本层无法感知重复释放（double free 会 abort）。
 */
void mdfile_free_string(char *ptr);

/*
 * 优化一份 Markdown。
 * out_json 输出：
 *   {
 *     "engine_version": "...", "preset": "...",
 *     "body": "<优化后语义层正文, LF>",
 *     "rendered_latin1": "<按格式层渲染的完整文件字节, latin1 逐字节编码>",
 *     "newline_style": "preserve|lf|crlf|cr", "had_bom": bool,
 *     "passes": n, "content_changes": 0, "structural_changes": n,
 *     "applied": [{"rule_id","rule_name","scope","edits","reason"}],
 *     "rejected": [{"rule_id","rule_name","reason"}],
 *     "categories": [["标题调整", n], ...],
 *     "score_before": {...}, "score_after": {...},
 *     "manifest_json": "...", "checksum_original": u64, "checksum_optimized": u64,
 *     "checksum_original_str": "...", "checksum_optimized_str": "..."
 *   }
 * content_changes != 0 时**不得**落盘；checksum_original == checksum_optimized 是「正文未变」的
 * 机器可验证证据，五平台同一输入必须得到同一对校验和。
 * 校验和是 64 位无符号数，17~19 位十进制超过 2^53：JS/WebView 侧请比对 *_str，
 * 不要用 JSON.parse 后的数字（会丢末位，导致「看起来一致」的假阳性）。
 */
int32_t mdfile_optimize(
    const uint8_t *input, size_t input_len,
    const char *preset,      /* NULL = 通用文章 */
    const char *newline,     /* NULL = preserve */
    char **out_json, size_t *out_len);

/* 只分析不改字。评分仅作提示，任何情况下不会驱动正文改写 */
int32_t mdfile_analyze(
    const uint8_t *input, size_t input_len,
    const char *preset,
    char **out_json, size_t *out_len);

/* 规则与预设清单（设置页、单规则测试） */
int32_t mdfile_rules_json(char **out_json, size_t *out_len);

/* 规则干跑，不写盘。rule_id == NULL 表示全部规则 */
int32_t mdfile_preview(
    const uint8_t *input, size_t input_len,
    const char *preset, const char *rule_id,
    char **out_json, size_t *out_len);

/*
 * 逐条接受 / 拒绝后重建正文（可逆性保证）。
 * decisions_json 形如 [{"decision":"reject","rule_id":"R01.blank-lines"}]
 * decision ∈ accept|reject|undo|redo|restore
 * 返回 {"body","rendered_latin1","newline_style","had_bom","content_changes","checksum","checksum_str"}：
 *   body 是 LF 语义层正文；**rendered_latin1 才是可直接写盘的完整文件字节**（含 BOM 与原换行风格），
 *   与 mdfile_optimize 的口径一致，避免 UI 在「预览」和「接受」两条路径写出不同字节。
 * content_changes 由 Content Guard 实际比对得出（不再是常量 0）；
 * 重建结果同样通过全量 Content Guard 才返回，否则 MDFILE_ERR_GUARD 且不返回任何内容。
 */
int32_t mdfile_rebuild(
    const uint8_t *original, size_t original_len,
    const char *preset, const char *decisions_json,
    char **out_json, size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif /* MDFILE_H */
