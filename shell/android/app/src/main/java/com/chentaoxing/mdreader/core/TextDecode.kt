package com.chentaoxing.mdreader.core

import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction
import java.nio.charset.StandardCharsets

/**
 * 显示层转码（与 Windows 壳 Shell/TextDecode.cs 同一套顺序）：
 * 1. 含 NUL 的字节流直接拒读（堵「无 BOM 的 UTF-16LE 恰好是合法 UTF-8」的洞）
 * 2. BOM 明确按 BOM（UTF-8 / UTF-16LE / UTF-16BE）
 * 3. 严格 UTF-8 解得出来就是 UTF-8
 * 4. 再试 GB18030（GBK 的超集）
 * 5. 都不行就明说认不出
 *
 * 转码只发生在内存里——原文件一个字节都不动，另存出去的永远是 UTF-8 新文件。
 */
object TextDecode {
    data class Decoded(val utf8: ByteArray, val label: String, val transcoded: Boolean)

    fun decode(bytes: ByteArray): Decoded? {
        if (bytes.indexOf(0) >= 0) return null

        // BOM
        if (bytes.size >= 3 && bytes[0] == 0xEF.toByte() && bytes[1] == 0xBB.toByte() && bytes[2] == 0xBF.toByte()) {
            return Decoded(strip(bytes, 3), "UTF-8（有 BOM）", false)
        }
        if (bytes.size >= 2 && bytes[0] == 0xFF.toByte() && bytes[1] == 0xFE.toByte()) {
            return Decoded(utf16To8(bytes, 2, little = true), "UTF-16LE", true)
        }
        if (bytes.size >= 2 && bytes[0] == 0xFE.toByte() && bytes[1] == 0xFF.toByte()) {
            return Decoded(utf16To8(bytes, 2, little = false), "UTF-16BE", true)
        }

        // 严格 UTF-8
        if (decodeStrictUtf8(bytes) != null) return Decoded(bytes, "UTF-8", false)

        // GB18030
        return try {
            val text = String(bytes, charset("GB18030"))
            Decoded(text.toByteArray(StandardCharsets.UTF_8), "GB18030（含 GBK，按内容猜的编码，若乱码请人工确认）", true)
        } catch (e: Exception) {
            null
        }
    }

    private fun decodeStrictUtf8(bytes: ByteArray): String? = try {
        val dec = StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
        dec.decode(java.nio.ByteBuffer.wrap(bytes)).toString()
    } catch (e: CharacterCodingException) {
        null
    }

    private fun strip(bytes: ByteArray, n: Int): ByteArray = bytes.copyOfRange(n, bytes.size)

    private fun utf16To8(bytes: ByteArray, from: Int, little: Boolean): ByteArray {
        val cs = if (little) StandardCharsets.UTF_16LE else StandardCharsets.UTF_16BE
        return String(bytes, from, bytes.size - from, cs).toByteArray(StandardCharsets.UTF_8)
    }
}
