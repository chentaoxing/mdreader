package com.chentaoxing.mdreader.core

import org.json.JSONObject

/** JNI 信封解析结果。 */
sealed class Envelope {
    data class Ok(val payload: JSONObject) : Envelope()
    data class Err(val status: Int, val error: String) : Envelope()

    companion object {
        fun parse(raw: String): Envelope {
            val o = JSONObject(raw)
            val st = o.optInt("status", -1)
            return if (st == 0) Ok(o.getJSONObject("payload")) else Err(st, o.optString("error", ""))
        }
    }
}

/** mdfile_optimize 的 payload（字段口径见 mdfile.h 注释）。 */
data class OptimizeResult(
    val preset: String,
    /** 优化后语义层正文（LF、无 BOM）——阅读视图渲染的输入 */
    val body: String,
    /** 按格式层渲染的完整文件字节（latin1 解码后），可直接另存 */
    val rendered: ByteArray,
    val newlineStyle: String,
    val hadBom: Boolean,
    val passes: Int,
    val contentChanges: Int,
    val structuralChanges: Int,
    val applied: List<AppliedRule>,
    val rejected: List<RejectedRule>,
    /** 综合分（优化后） */
    val scoreOverall: Int,
    val scoreBeforeOverall: Int,
    val checksumOriginalStr: String,
    val checksumOptimizedStr: String,
) {
    /** 与 mdfile.h 同一条铁律：校验和按字符串比对（u64 超 2^53，数值比较会丢位）。 */
    val contentUntouched: Boolean
        get() = contentChanges == 0 && checksumOriginalStr.length > 0 &&
            checksumOriginalStr == checksumOptimizedStr
}

data class AppliedRule(
    val ruleId: String,
    val ruleName: String,
    val scope: String,
    val edits: Int,
    val reason: String,
)

data class RejectedRule(val ruleId: String, val ruleName: String, val reason: String)

/** mdfile_rebuild 的 payload。 */
data class RebuildResult(
    val body: String,
    val rendered: ByteArray,
    val newlineStyle: String,
    val hadBom: Boolean,
    val contentChanges: Int,
    val checksumStr: String,
)

object Wire {
    /** latin1 逐字节还原：JSON 里第 n 个字符 = 文件第 n 个字节（mdfile.h 契约）。 */
    fun latin1ToBytes(s: String): ByteArray {
        val out = ByteArray(s.length)
        for (i in s.indices) out[i] = (s[i].code and 0xFF).toByte()
        return out
    }

    fun optimize(p: JSONObject): OptimizeResult {
        // 先解 payload 里的评分与校验和
        val after = p.optJSONObject("score_after") ?: JSONObject()
        val before = p.optJSONObject("score_before") ?: JSONObject()
        return OptimizeResult(
            preset = p.optString("preset"),
            body = p.optString("body"),
            rendered = latin1ToBytes(p.optString("rendered_latin1")),
            newlineStyle = p.optString("newline_style", "preserve"),
            hadBom = p.optBoolean("had_bom", false),
            passes = p.optInt("passes", 1),
            contentChanges = p.optInt("content_changes", -1),
            structuralChanges = p.optInt("structural_changes", 0),
            applied = (p.optJSONArray("applied") ?: org.json.JSONArray()).let { a ->
                (0 until a.length()).map {
                    val o = a.getJSONObject(it)
                    AppliedRule(
                        o.getString("rule_id"), o.getString("rule_name"),
                        o.getString("scope"), o.getInt("edits"), o.optString("reason"),
                    )
                }
            },
            rejected = (p.optJSONArray("rejected") ?: org.json.JSONArray()).let { a ->
                (0 until a.length()).map {
                    val o = a.getJSONObject(it)
                    RejectedRule(o.getString("rule_id"), o.getString("rule_name"), o.optString("reason"))
                }
            },
            scoreOverall = after.optInt("overall", -1),
            scoreBeforeOverall = before.optInt("overall", -1),
            checksumOriginalStr = p.optString("checksum_original_str"),
            checksumOptimizedStr = p.optString("checksum_optimized_str"),
        )
    }

    fun rebuild(p: JSONObject): RebuildResult = RebuildResult(
        body = p.optString("body"),
        rendered = latin1ToBytes(p.optString("rendered_latin1")),
        newlineStyle = p.optString("newline_style", "preserve"),
        hadBom = p.optBoolean("had_bom", false),
        contentChanges = p.optInt("content_changes", -1),
        checksumStr = p.optString("checksum_str"),
    )

    fun presets(rules: JSONObject): List<String> {
        val arr = rules.optJSONArray("presets") ?: return emptyList()
        return (0 until arr.length()).map { arr.getJSONArray(it).getString(0) }
    }

    fun statusText(code: Int): String = when (code) {
        0 -> "成功"
        1 -> "输入不是合法 UTF-8（核心绝不猜编码）"
        2 -> "输入超过 64 MiB 上限"
        3 -> "Content Guard 拒绝：正文发生变化，结果已丢弃"
        4 -> "未知预设 / 未知规则 / 非法参数"
        5 -> "核心内部异常"
        -1 -> "空指针或长度越界"
        else -> "未知状态 $code"
    }
}
