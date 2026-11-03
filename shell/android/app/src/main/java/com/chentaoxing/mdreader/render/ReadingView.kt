package com.chentaoxing.mdreader.render

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * 阅读视图（纯显示层，移植自 Windows 壳 ReadingRenderer 的读法）。
 *
 * 铁律：这一层只决定**屏幕上怎么摆**——硬换行接回整句、行长封顶、标题节奏、
 * 列表悬挂缩进、表格画成真表格、任务项渲染成 ☑/☐、front matter 收成元信息块。
 * 要不要改、改哪儿，全部来自核心返回的 body；这里没有任何写文件的路径。
 */

private sealed class Blk {
    data class Meta(val lines: List<String>) : Blk()
    data class Head(val level: Int, val text: String) : Blk()
    data class Para(val lines: List<String>) : Blk()
    data class Quote(val depth: Int, val text: String) : Blk()
    data class Item(val indent: Int, val task: String?, val text: String) : Blk()
    data class Code(val lang: String, val lines: List<String>) : Blk()
    data class Table(val header: List<String>, val rows: List<List<String>>, val aligns: List<String>) : Blk()
    object Rule : Blk()
}

private fun parseBlocks(body: String): List<Blk> {
    val out = mutableListOf<Blk>()
    val lines = body.lines()
    var i = 0

    // front matter（核心已规范化；这里只做显示层的「收成元信息块」）
    if (lines.isNotEmpty() && (lines[0] == "---" || lines[0] == "+++")) {
        val fence = lines[0]
        var j = 1
        val meta = mutableListOf<String>()
        while (j < lines.size && lines[j] != fence) { meta.add(lines[j]); j++ }
        if (j < lines.size) { // 有闭合才算 front matter
            out.add(Blk.Meta(meta))
            i = j + 1
        }
    }

    var para = mutableListOf<String>()
    fun flushPara() {
        if (para.isNotEmpty()) { out.add(Blk.Para(para)); para = mutableListOf() }
    }

    while (i < lines.size) {
        val l = lines[i]
        val trimmed = l.trimStart()
        val indent = l.length - trimmed.length

        when {
            l.isBlank() -> { flushPara() }
            trimmed.startsWith("```") || trimmed.startsWith("~~~") -> {
                flushPara()
                val fence = trimmed.take(3)
                val lang = trimmed.drop(3).trim()
                val code = mutableListOf<String>()
                var j = i + 1
                while (j < lines.size && !lines[j].trimStart().startsWith(fence)) { code.add(lines[j]); j++ }
                out.add(Blk.Code(lang, code))
                i = j
            }
            trimmed.startsWith("#") -> {
                flushPara()
                val m = Regex("^(#{1,6})\\s*(.+?)(\\s+#+)?\\s*$").find(trimmed)
                if (m != null) out.add(Blk.Head(m.groupValues[1].length, m.groupValues[2]))
                else out.add(Blk.Para(listOf(l)))
            }
            trimmed == "---" || trimmed == "***" || trimmed == "___" -> {
                flushPara(); out.add(Blk.Rule)
            }
            trimmed.startsWith(">") -> {
                flushPara()
                val depth = trimmed.takeWhile { it == '>' }.length
                out.add(Blk.Quote(depth, trimmed.dropWhile { it == '>' }.removePrefix(" ")))
            }
            isListMarker(trimmed) -> {
                flushPara()
                val m = listMarker(trimmed)!!
                val rest = trimmed.substring(m.length).trimStart()
                val task = Regex("^\\[([ xX])]\\s+(.*)$").find(rest)?.let { it.groupValues[1] }
                val text = if (task != null) Regex("^\\[([ xX])]\\s+(.*)$").find(rest)!!.groupValues[2] else rest
                out.add(Blk.Item(indent, task, text))
            }
            l.contains("|") && i + 1 < lines.size && isTableDelimiter(lines[i + 1].trim()) -> {
                flushPara()
                val header = splitRow(trimmed)
                val aligns = splitRow(lines[i + 1].trim()).map { c ->
                    when {
                        c.startsWith(":") && c.endsWith(":") -> "center"
                        c.endsWith(":") -> "end"
                        else -> "start"
                    }
                }
                val rows = mutableListOf<List<String>>()
                var j = i + 2
                while (j < lines.size && lines[j].contains("|") && lines[j].isNotBlank()) {
                    rows.add(splitRow(lines[j].trim())); j++
                }
                out.add(Blk.Table(header, rows, aligns))
                i = j - 1
            }
            else -> {
                // 段落里的硬换行：接回整句（阅读视图的核心动作）
                if (trimmed.startsWith("    ") && para.isEmpty()) {
                    // 四空格缩进空行首 → 代码（保守按代码显示）
                    out.add(Blk.Code("", listOf(trimmed.drop(4))))
                } else {
                    para.add(trimmed)
                }
            }
        }
        i++
    }
    flushPara()
    return out
}

private fun isListMarker(t: String): Boolean = listMarker(t) != null
private fun listMarker(t: String): String? = listOf("- ", "* ", "+ ").firstOrNull { t.startsWith(it) }
    ?: Regex("^\\d+\\. ").find(t)?.value

private fun isTableDelimiter(t: String): Boolean =
    t.contains("-") && t.replace("|", "").replace("-", "").replace(":", "").replace(" ", "").isEmpty() && t.contains("|")

private fun splitRow(t: String): List<String> {
    var s = t.trim()
    if (s.startsWith("|")) s = s.drop(1)
    if (s.endsWith("|")) s = s.dropLast(1)
    return s.split("|").map { it.trim() }
}

@Composable
fun ReadingView(body: String, modifier: Modifier = Modifier, mode: SymbolMode = SymbolMode.Dim) {
    val blocks = parseBlocks(body)
    val mono = FontFamily.Monospace
    val linkColor = MaterialTheme.colorScheme.primary
    val dimColor = MaterialTheme.colorScheme.outlineVariant
    val codeBg = MaterialTheme.colorScheme.surfaceVariant
    Column(modifier = modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        blocks.forEach { b ->
            when (b) {
                is Blk.Meta -> MetaBlock(b.lines)
                is Blk.Head -> InlineText(
                    text = b.text, mode = mode, linkColor = linkColor, dimColor = dimColor, codeBg = codeBg,
                    style = when (b.level) {
                        1 -> MaterialTheme.typography.headlineMedium
                        2 -> MaterialTheme.typography.headlineSmall
                        3 -> MaterialTheme.typography.titleLarge
                        else -> MaterialTheme.typography.titleMedium
                    },
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(top = 20.dp, bottom = 6.dp),
                )
                is Blk.Para -> InlineText(
                    text = b.lines.joinToString(""), mode = mode,
                    linkColor = linkColor, dimColor = dimColor, codeBg = codeBg,
                    style = MaterialTheme.typography.bodyLarge.copy(lineHeight = 28.sp),
                    modifier = Modifier.padding(vertical = 6.dp),
                )
                is Blk.Quote -> Row(
                    Modifier.padding(vertical = 4.dp, horizontal = (6 * (b.depth - 1)).dp).height(IntrinsicSize.Min)
                ) {
                    Box(Modifier.padding(end = 8.dp).width(3.dp).fillMaxHeight()
                        .clip(RoundedCornerShape(2.dp))
                        .background(MaterialTheme.colorScheme.outlineVariant))
                    InlineText(
                        text = b.text, mode = mode, linkColor = linkColor, dimColor = dimColor, codeBg = codeBg,
                        style = MaterialTheme.typography.bodyLarge.copy(
                            color = MaterialTheme.colorScheme.onSurfaceVariant, lineHeight = 26.sp),
                    )
                }
                is Blk.Item -> Row(Modifier.padding(start = (b.indent * 6).dp, top = 3.dp, bottom = 3.dp)) {
                    val mark = when (b.task) {
                        "x", "X" -> "☑"
                        " " -> "☐"
                        null -> "•"
                        else -> "•"
                    }
                    Text(mark, modifier = Modifier.width(22.dp),
                        color = if (b.task != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)
                    InlineText(
                        text = b.text, mode = mode, linkColor = linkColor, dimColor = dimColor, codeBg = codeBg,
                        style = MaterialTheme.typography.bodyLarge.copy(lineHeight = 26.sp),
                    )
                }
                is Blk.Code -> Surface(
                    color = MaterialTheme.colorScheme.surfaceVariant,
                    shape = RoundedCornerShape(8.dp),
                    modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
                ) {
                    Column(Modifier.padding(10.dp)) {
                        if (b.lang.isNotEmpty()) Text(
                            b.lang, style = TextStyle(fontFamily = mono, fontSize = 11.sp),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Text(
                            b.lines.joinToString("\n"),
                            style = TextStyle(fontFamily = mono, fontSize = 13.sp, lineHeight = 20.sp),
                        )
                    }
                }
                is Blk.Table -> TableBlock(b)
                Blk.Rule -> HorizontalDivider(
                    modifier = Modifier.padding(vertical = 14.dp),
                    color = MaterialTheme.colorScheme.outlineVariant,
                )
            }
        }
        Spacer(Modifier.height(40.dp))
    }
}

@Composable
private fun MetaBlock(lines: List<String>) {
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.4f),
        shape = RoundedCornerShape(8.dp),
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
    ) {
        Column(Modifier.padding(10.dp)) {
            Text("元信息（front matter）", style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            lines.forEach {
                Text(it, style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 18.sp))
            }
        }
    }
}

@Composable
private fun TableBlock(b: Blk.Table) {
    val hScroll = rememberScrollState()
    Column(
        Modifier.fillMaxWidth().padding(vertical = 6.dp)
            .clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.surfaceVariant)
            .horizontalScroll(hScroll).padding(8.dp),
    ) {
        Row {
            b.header.forEachIndexed { ci, c ->
                Text(c, fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.widthIn(min = 72.dp).padding(horizontal = 8.dp, vertical = 4.dp))
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline)
        b.rows.forEach { r ->
            Row {
                b.header.indices.forEach { ci ->
                    Text(r.getOrElse(ci) { "" }, style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.widthIn(min = 72.dp).padding(horizontal = 8.dp, vertical = 4.dp))
                }
            }
        }
    }
}

/** 段落级文本：走 InlineRenderer 三档行内渲染（原样/弱化/隐藏）。 */
@Composable
private fun InlineText(
    text: String,
    mode: SymbolMode,
    linkColor: androidx.compose.ui.graphics.Color,
    dimColor: androidx.compose.ui.graphics.Color,
    codeBg: androidx.compose.ui.graphics.Color,
    style: TextStyle,
    modifier: Modifier = Modifier,
    fontWeight: FontWeight? = null,
) {
    Text(
        InlineRenderer.render(text, mode, linkColor, dimColor, codeBg),
        style = style, fontWeight = fontWeight, modifier = modifier,
    )
}
