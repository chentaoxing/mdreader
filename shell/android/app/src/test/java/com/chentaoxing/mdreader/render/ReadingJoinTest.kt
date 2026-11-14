package com.chentaoxing.mdreader.render

import org.junit.Assert.assertEquals
import org.junit.Test

// 被测对象是 render/ReadingView.kt 正本里的 joinPara。
// 用例与 Windows 壳 tests/reading-join 的 14 项一一对应（2026-11-05 三条修复的安卓侧镜像）。
class ReadingJoinTest {
    private fun j(vararg lines: String): String = joinPara(lines.toList())

    @Test fun `字段行成对出现 每行各占一行`() {
        val fields = listOf(
            "标题：他低头靠在护栏上，路边有人在喊他",
            "封面小字：那天好多人举着手机",
            "封面大字：喊他一声",
            "话题标签：#殡葬 #摆渡人辉哥 #人间百态 #长沙 #骑手 #敬畏生命",
            "置顶提问：你在路上有没有为一个人停过车？停下的那一次，你还记得什么",
            "发布窗口：19:30-20:00",
        )
        assertEquals(fields.joinToString("\n"), joinPara(fields))
    }

    @Test fun `顿号收尾 保留换行`() {
        assertEquals(
            "入选标准：强自洽、\nhook 信息密度高、\n能引发转发或评论争议",
            j("入选标准：强自洽、",
              "hook 信息密度高、",
              "能引发转发或评论争议"),
        )
    }

    @Test fun `分号收尾 保留换行`() {
        assertEquals("第一条；\n第二条", j("第一条；", "第二条"))
    }

    @Test fun `中文句中被折开 接成一行且不补空格`() {
        assertEquals(
            "我是辉哥，干殡葬三十多年了。前几天看到一条视频。",
            j("我是辉哥，干殡葬三十多年", "了。前几天看到一条视频。"),
        )
    }

    @Test fun `中英交界处被折开 接回去补一个空格`() {
        assertEquals("这条视频的 hook 信息密度很高", j("这条视频的 hook", "信息密度很高"))
    }

    @Test fun `上一行不是字段行 接回去`() {
        assertEquals(
            "以下每条含发布文案和定稿口播改动说明附在每条末尾。",
            j("以下每条含发布文案和定稿口播", "改动说明附在每条末尾。"),
        )
    }

    @Test fun `数字开头的冒号不算字段行`() {
        assertEquals("10:30 集合然后开始拍摄", j("10:30 集合", "然后开始拍摄"))
    }

    @Test fun `URL 里的冒号不算字段行`() {
        assertEquals("http://a.com/x：y 参考这个链接", j("http://a.com/x：y", "参考这个链接"))
    }

    @Test fun `标签里有时间戳 该行仍算字段行`() {
        assertEquals("标题：靠护栏的骑手\n发布窗口：19:30-20:00", j("标题：靠护栏的骑手", "发布窗口：19:30-20:00"))
    }

    @Test fun `行尾两空格 硬换行照旧`() {
        assertEquals("第一行\n第二行", j("第一行  ", "第二行"))
    }

    @Test fun `行尾单反斜杠 换行且记号不上屏`() {
        assertEquals("第一行\n第二行", j("第一行\\", "第二行"))
    }

    @Test fun `行尾双反斜杠 是字面反斜杠 不分行`() {
        assertEquals("末段\\\\ 第二行", j("末段\\\\", "第二行"))
    }

    @Test fun `单行 原样返回`() {
        assertEquals("只有一行", j("只有一行"))
    }

    @Test fun `空列表 返回空串`() {
        assertEquals("", joinPara(emptyList()))
    }
}
