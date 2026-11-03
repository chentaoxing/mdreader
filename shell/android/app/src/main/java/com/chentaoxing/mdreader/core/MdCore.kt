package com.chentaoxing.mdreader.core

/**
 * mdfile.h C ABI 的 Android 侧唯一入口（经 libmdcore_jni.so 的 Rust 胶水）。
 *
 * 每个方法返回一个「信封」JSON 字符串：
 *   {"status":0, "payload": {…mdfile.h 定义的原样 JSON…}}
 *   {"status":n, "error":"…"}        // 状态码表见 mdfile.h
 *
 * 排版判定一行不在 Kotlin 侧——要不要改、改哪儿、能不能改，全部以核心返回的 JSON 为准。
 */
object MdCore {
    init {
        // 胶水库依赖 libmdcore_ffi.so（DT_NEEDED），加载时由链接器一并解析
        System.loadLibrary("mdcore_jni")
    }

    val engineVersion: String by lazy { engineVersionNative() }

    /** 优化。newline 取 "preserve"|"lf"|"crlf"|"cr"，null = preserve。 */
    external fun optimize(input: ByteArray, preset: String?, newline: String?): String

    /** 只读评分。 */
    external fun analyze(input: ByteArray, preset: String?): String

    /** 规则干跑（ruleId == null 表示全部规则）。 */
    external fun preview(input: ByteArray, preset: String?, ruleId: String?): String

    /** 逐条接受/拒绝后重建。decisionsJson 形如 [{"decision":"reject","rule_id":"R01…"}]。 */
    external fun rebuild(original: ByteArray, preset: String?, decisionsJson: String): String

    /** 规则与预设清单。 */
    external fun rulesJson(): String

    private external fun engineVersionNative(): String
}
