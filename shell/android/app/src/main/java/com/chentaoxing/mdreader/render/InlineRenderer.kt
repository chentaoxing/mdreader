package com.chentaoxing.mdreader.render

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration

/**
 * 行内语法渲染（纯显示层——文件一个字节不动）。
 *
 * 与 Windows 壳 ReadingRenderer 的三档口径一致：
 *  - Raw    「原样」：语法符号全部上屏
 *  - Dim    「弱化」：`**加粗**` 渲染为粗体且 ** 淡出、`code` 等宽加底色、[t](u) 只亮文字
 *  - Hidden 「隐藏」：同弱化，但语法符号与 URL 完全不上屏
 *
 * 只处理能可靠配对的三种行内语法：**bold** / *em* / `code` / [text](url)；
 * 未配对的一律按字面上屏（宁可少渲染，不可错渲染）。
 */
enum class SymbolMode(val label: String) { Raw("原样"), Dim("弱化"), Hidden("隐藏") }

object InlineRenderer {

    fun render(text: String, mode: SymbolMode, linkColor: Color, dimColor: Color, codeBg: Color): AnnotatedString {
        if (mode == SymbolMode.Raw || !text.contains('*') && !text.contains('`') && !text.contains('[')) {
            return AnnotatedString(text)
        }
        val sb = StringBuilder(text.length + 16)
        val spans = mutableListOf<Triple<Int, Int, SpanStyle>>()
        var i = 0
        var last = 0

        fun pushPlain(from: Int, to: Int) {
            if (to > from) sb.append(text, from, to)
        }
        fun span(from: Int, style: SpanStyle) {
            spans.add(Triple(from, sb.length, style))
        }

        while (i < text.length) {
            when (text[i]) {
                '`' -> {
                    val close = text.indexOf('`', i + 1)
                    if (close > i + 1) {
                        pushPlain(last, i)
                        val start = sb.length
                        sb.append(text, i + 1, close)
                        span(start, SpanStyle(fontFamily = FontFamily.Monospace, background = codeBg))
                        i = close + 1; last = i
                    } else i++
                }
                '*' -> {
                    val isBold = text.startsWith("**", i)
                    val marker = if (isBold) 2 else 1
                    val close = findClose(text, i + marker, if (isBold) "**" else "*")
                    if (close > 0) {
                        pushPlain(last, i)
                        val inner = text.substring(i + marker, close)
                        val style = if (isBold) SpanStyle(fontWeight = FontWeight.Bold)
                                    else SpanStyle(fontStyle = FontStyle.Italic)
                        if (mode == SymbolMode.Dim) {
                            // 符号上屏但淡出：open 标记 + 内容 + close 标记，各自上色
                            var s = sb.length
                            sb.append(text, i, i + marker)
                            span(s, SpanStyle(color = dimColor))
                            s = sb.length
                            sb.append(inner)
                            span(s, style)
                            s = sb.length
                            sb.append(text, close, close + marker)
                            span(s, SpanStyle(color = dimColor))
                        } else {
                            // Hidden：符号不上屏，只渲染内容
                            val s = sb.length
                            sb.append(inner)
                            span(s, style)
                        }
                        i = close + marker; last = i
                    } else i++
                }
                '[' -> {
                    val close = text.indexOf(']', i)
                    val openParen = if (close > 0 && close + 1 < text.length && text[close + 1] == '(') close + 1 else -1
                    val closeParen = if (openParen > 0) text.indexOf(')', openParen) else -1
                    if (closeParen > openParen + 1) {
                        pushPlain(last, i)
                        val label = text.substring(i + 1, close)
                        val url = text.substring(openParen + 1, closeParen)
                        val s = sb.length
                        sb.append(label)
                        span(s, SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline))
                        if (mode == SymbolMode.Dim) {
                            val s2 = sb.length
                            sb.append('('); sb.append(url); sb.append(')')
                            span(s2, SpanStyle(color = dimColor))
                        }
                        // Hidden：URL 完全不上屏
                        i = closeParen + 1; last = i
                    } else i++
                }
                else -> i++
            }
        }
        pushPlain(last, text.length)

        return buildAnnotatedString {
            append(sb.toString())
            spans.forEach { (f, t, st) -> addStyle(st, f, t) }
        }
    }

    /** 从 from 起找配对闭合标记；不支持嵌套同类标记（最小可靠子集）。 */
    private fun findClose(s: String, from: Int, marker: String): Int {
        var i = from
        while (i + marker.length <= s.length) {
            if (s.startsWith(marker, i)) return i
            i++
        }
        return -1
    }
}
