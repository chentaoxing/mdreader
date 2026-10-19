using System.Text;
using System.Text.RegularExpressions;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

namespace MdFileAssistant.Reading;

/// <summary>
/// 阅读模式：把一份 Markdown 在**屏幕上**排成给人读的样子。
///
/// 铁律：本类只产视图，没有任何写文件的路径。落盘字节永远来自核心返回的 rendered，
/// 与这里生成的 FlowDocument 没有因果关系；正文是否被改动由核心的校验和证明，不由这里。
///
///   Raw    语法字符原样上屏（版式仍是给人排好的：段宽、节奏、不硬折行）
///   Dim    语法字符还在，只是淡到不抢注意力
///   Hidden 语法字符不上屏：`**粗**` 直接粗体，表格画成真表格，`#` 变成标题本身
///
/// 为什么连版式一起做而不只是「藏符号」：人的眼睛靠行长、段落间距和标题层级定位。
/// 把这些交给屏幕、而不是塞进文件字节，才是这个工具能安全做的事。
/// </summary>
public enum ReadingMode { Raw, Dim, Hidden }

public static class ReadingRenderer
{
    /// <summary>超过这个长度不做富文本（几 MB 的 FlowDocument 会把界面拖死），降级为纯文本上屏。</summary>
    public const int MaxRichChars = 800_000;

    private static readonly FontFamily TextFam = new("Microsoft YaHei UI, Segoe UI");
    private static readonly FontFamily Mono = new("Consolas, Cascadia Mono, Courier New");

    private static readonly Brush BodyInk = Frozen(Color.FromRgb(0x22, 0x29, 0x2F));
    private static readonly Brush HeadInk = Frozen(Color.FromRgb(0x0B, 0x3B, 0x5C));
    private static readonly Brush MarkupBrush = Frozen(Color.FromRgb(0xAE, 0xB4, 0xBD));
    private static readonly Brush MetaBrush = Frozen(Color.FromRgb(0x8A, 0x90, 0x99));
    private static readonly Brush CodeInk = Frozen(Color.FromRgb(0x14, 0x4A, 0x6E));
    private static readonly Brush CodeBack = Frozen(Color.FromRgb(0xF2, 0xF5, 0xF8));
    private static readonly Brush QuoteInk = Frozen(Color.FromRgb(0x45, 0x52, 0x5C));
    private static readonly Brush QuoteBack = Frozen(Color.FromRgb(0xF7, 0xF9, 0xFB));
    private static readonly Brush QuoteBar = Frozen(Color.FromRgb(0xC3, 0xD2, 0xE0));
    private static readonly Brush LineBrush = Frozen(Color.FromRgb(0xD8, 0xDE, 0xE6));
    private static readonly Brush CellHead = Frozen(Color.FromRgb(0xEE, 0xF3, 0xF7));
    private static readonly Brush CellAlt = Frozen(Color.FromRgb(0xFA, 0xFB, 0xFD));
    private static readonly Brush CellLine = Frozen(Color.FromRgb(0xDD, 0xE3, 0xE9));
    private static readonly Brush LinkInk = Frozen(Color.FromRgb(0x14, 0x53, 0x2D));

    private static SolidColorBrush Frozen(Color c) { var b = new SolidColorBrush(c); b.Freeze(); return b; }

    private enum Kind { Para, Head, Quote, Item, Code, Table, Rule, Meta }

    /// <summary>一个块：只有类型和源文本行，不含任何「该不该改文件」的判断。</summary>
    private sealed class Blk
    {
        public Kind Kind;
        public int Level;                       // 标题层级 / 引用深度 / 列表缩进层
        public string Marker = "";              // 列表符号或标题井号原文
        public string Lang = "";                // 围栏语言
        public string Task = "";                // 任务列表的 [ ] / [x] 原文
        public bool Loose;                      // 列表项之间有空行
        public readonly List<string> Lines = new();
        public readonly List<string> Header = new();
        public readonly List<string> Rows = new();
        public readonly List<string> Aligns = new();
    }

    public static FlowDocument RenderReading(string text, ReadingMode mode, double scale)
    {
        var sc = scale <= 0 ? 1.0 : scale;
        var doc = NewDoc(sc);
        AutomationProperties.SetName(doc, "阅读视图");

        if (text.Length > MaxRichChars)
        {
            doc.Blocks.Add(new Paragraph(new Run($"（超过 {MaxRichChars / 1000} 千字符，阅读视图降级为纯文本以免卡顿；文件本身一个字节都没动）"))
            { Foreground = MetaBrush, FontStyle = FontStyles.Italic, FontSize = Px(13, sc), Margin = new Thickness(0, 0, 0, 12) });
            doc.Blocks.Add(new Paragraph(new Run(text)) { FontSize = Px(14, sc), Margin = new Thickness(0) });
            return doc;
        }

        foreach (var b in Parse(Normalize(text), mode))
        {
            switch (b.Kind)
            {
                case Kind.Head: AddHeading(doc, b, mode, sc); break;
                case Kind.Para: AddParagraph(doc, b, mode, sc); break;
                case Kind.Quote: AddQuote(doc, b, mode, sc); break;
                case Kind.Item: AddItem(doc, b, mode, sc); break;
                case Kind.Code: AddCode(doc, b, sc); break;
                case Kind.Table: AddTable(doc, b, mode, sc); break;
                case Kind.Rule: AddRule(doc, sc); break;
                case Kind.Meta: AddMeta(doc, b, sc); break;
            }
        }
        if (doc.Blocks.Count == 0)
            doc.Blocks.Add(new Paragraph(new Run("（这份文件没有正文）")) { Foreground = MetaBrush, FontSize = Px(13, sc) });
        return doc;
    }

    private static FlowDocument NewDoc(double sc) => new()
    {
        FontFamily = TextFam,
        FontSize = Px(15.5, sc),
        Foreground = BodyInk,
        LineHeight = Px(15.5, sc) * 1.85,
        LineStackingStrategy = LineStackingStrategy.BlockLineHeight,
        PagePadding = new Thickness(22, 18, 22, 30),
        TextAlignment = TextAlignment.Left,
        // 行长上限：窗口再宽也不让一行跑到眼睛跟不上的地方。这是阅读视图唯一的「版式硬约束」。
        // 按标准字号算约 40 个汉字一行；字号放大时上限同步放大，保持同样的字数。
        MaxPageWidth = 880 * sc,
        ColumnWidth = 880 * sc,
    };

    private static double Px(double v, double sc) => Math.Round(v * sc * 2) / 2;

    private static string[] Normalize(string s) => s.Replace("\r\n", "\n").Replace('\r', '\n').Split('\n');

    private static bool IsBlank(string l) => l.Trim().Length == 0;

    // ---- 分块 ------------------------------------------------------------

    private static readonly Regex AtxRx = new(@"^[ ]{0,3}(?<mk>#{1,6})(?:[ ]+(?<rest>.*?))?[ ]*$", C());
    private static readonly Regex FenceRx = new(@"^[ ]{0,3}(?:```|~~~)[ ]*(?<info>[^`]*)$", C());
    private static readonly Regex QuoteRx = new(@"^(?<mk>[ ]*>+[ ]?)(?<rest>.*)$", C());
    private static readonly Regex ListRx = new(@"^(?<pad>[ ]*)(?<mk>[-*+]|[0-9]{1,3}[.)])(?<gap>[ ]+)(?<rest>.*)$", C());
    private static readonly Regex RuleRx = new(@"^[ ]*(?:[-*_])(?:[ ]*(?:[-*_])){2,}[ ]*$", C());
    private static readonly Regex TaskRx = new(@"^\[(?<s>[ xX])\][ ]+(?<rest>.*)$", C());
    private static readonly Regex TableDelimRx = new(@"^[ ]*\|?[ ]*:?-{2,}:?[ ]*(?:\|[ ]*:?-{2,}:?[ ]*)*\|?[ ]*$", C());
    private static readonly Regex TailHashRx = new(@"[ ]+#+[ ]*$", C());

    private static RegexOptions C() => RegexOptions.Compiled | RegexOptions.CultureInvariant;

    private static List<Blk> Parse(string[] ls, ReadingMode mode)
    {
        var outB = new List<Blk>();
        int i = 0, n = ls.Length;

        if (n > 0 && (ls[0].Trim() == "---" || ls[0].Trim() == "+++"))
        {
            int j = 1;
            while (j < n && ls[j].Trim() is not ("---" or "..." or "+++")) j++;
            var meta = new Blk { Kind = Kind.Meta };
            for (int k = 0; k <= Math.Min(j, n - 1); k++) meta.Lines.Add(ls[k]);
            outB.Add(meta);
            i = Math.Min(j + 1, n);
        }

        while (i < n)
        {
            var line = ls[i];
            if (IsBlank(line)) { i++; continue; }

            if (FenceRx.IsMatch(line))
            {
                var code = new Blk { Kind = Kind.Code, Lang = FenceRx.Match(line).Groups["info"].Value.Trim() };
                i++;
                while (i < n && !FenceRx.IsMatch(ls[i])) { code.Lines.Add(ls[i]); i++; }
                if (i < n) i++;
                outB.Add(code);
                continue;
            }

            var atx = AtxRx.Match(line);
            if (atx.Success)
            {
                var t = atx.Groups["rest"].Value;
                if (mode != ReadingMode.Raw) t = TailHashRx.Replace(t, "");
                outB.Add(new Blk { Kind = Kind.Head, Level = atx.Groups["mk"].Value.Length, Marker = atx.Groups["mk"].Value, Lines = { t } });
                i++;
                continue;
            }

            if (RuleRx.IsMatch(line)) { outB.Add(new Blk { Kind = Kind.Rule }); i++; continue; }

            if (QuoteRx.IsMatch(line))
            {
                var q = new Blk { Kind = Kind.Quote, Level = 1 };
                while (i < n && !IsBlank(ls[i]) && QuoteRx.IsMatch(ls[i]))
                {
                    var m = QuoteRx.Match(ls[i]);
                    q.Level = Math.Max(q.Level, m.Groups["mk"].Value.Count(c => c == '>'));
                    q.Lines.Add(m.Groups["rest"].Value);
                    i++;
                }
                outB.Add(q);
                continue;
            }

            if (ListRx.IsMatch(line))
            {
                while (i < n && ListRx.IsMatch(ls[i]))
                {
                    var m = ListRx.Match(ls[i]);
                    var item = new Blk
                    {
                        Kind = Kind.Item,
                        Level = Math.Min(5, m.Groups["pad"].Value.Length / 2),
                        Marker = m.Groups["mk"].Value + m.Groups["gap"].Value,
                    };
                    var rest = m.Groups["rest"].Value;
                    var task = TaskRx.Match(rest);
                    if (task.Success)
                    {
                        item.Task = "[" + task.Groups["s"].Value + "] ";
                        rest = task.Groups["rest"].Value;
                    }
                    item.Lines.Add(rest);
                    i++;

                    // 列表项的续行并进来，阅读时才不在句子中间断行
                    while (i < n && !IsBlank(ls[i]) && !StartsBlock(ls, i)) { item.Lines.Add(ls[i].Trim()); i++; }

                    int j = i, blanks = 0;
                    while (j < n && IsBlank(ls[j])) { blanks++; j++; }
                    if (blanks > 0 && j < n && ListRx.IsMatch(ls[j])) { item.Loose = true; i = j; }
                    outB.Add(item);
                }
                continue;
            }

            if (IsTableStart(ls, i))
            {
                var tb = new Blk { Kind = Kind.Table };
                foreach (var c in Cells(ls[i + 1])) tb.Aligns.Add(c.Trim());
                tb.Header.Add(ls[i]);
                i += 2;
                while (i < n && !IsBlank(ls[i]) && ls[i].Contains('|')) { tb.Rows.Add(ls[i]); i++; }
                outB.Add(tb);
                continue;
            }

            var para = new Blk { Kind = Kind.Para };
            while (i < n && !IsBlank(ls[i]))
            {
                // 第二行是 === 或 --- 时，这两行其实是一个 setext 标题（下划线只是语法）
                if (para.Lines.Count == 1)
                {
                    var under = SetextKind(ls[i]);
                    if (under != 0)
                    {
                        outB.Add(new Blk { Kind = Kind.Head, Level = under, Marker = "", Lines = { para.Lines[0] } });
                        para.Lines.Clear();
                        i++;
                        break;
                    }
                }
                if (StartsBlock(ls, i)) break;
                para.Lines.Add(ls[i]);
                i++;
            }
            if (para.Lines.Count > 0) outB.Add(para);
            if (para.Lines.Count == 0 && i < n && IsBlank(ls[i])) i++;
        }
        return outB;
    }

    private static bool StartsBlock(string[] ls, int i)
    {
        var l = ls[i];
        return FenceRx.IsMatch(l) || AtxRx.IsMatch(l) || QuoteRx.IsMatch(l) || ListRx.IsMatch(l) || RuleRx.IsMatch(l) || IsTableStart(ls, i);
    }

    /// <summary>setext 下划线：整行只有 = 或只有 -。返回标题层级，0 表示不是。</summary>
    private static int SetextKind(string l)
    {
        var t = l.Trim();
        if (t.Length == 0) return 0;
        if (t.All(c => c == '=')) return 1;
        if (t.Length >= 2 && t.All(c => c == '-')) return 2;
        return 0;
    }

    private static bool IsTableStart(string[] ls, int i)
    {
        if (i + 1 >= ls.Length || !ls[i].Contains('|')) return false;
        var d = ls[i + 1].Trim();
        return d.Contains('|') && TableDelimRx.IsMatch(d);
    }

    private static List<string> Cells(string line)
    {
        var s = line.Trim();
        if (s.StartsWith("|")) s = s[1..];
        if (s.EndsWith('|') && !s.EndsWith("\\|")) s = s[..^1];
        var cells = new List<string>();
        var cur = new StringBuilder();
        for (int k = 0; k < s.Length; k++)
        {
            if (s[k] == '\\' && k + 1 < s.Length && s[k + 1] == '|') { cur.Append('|'); k++; continue; }   // 转义竖线还原成内容
            if (s[k] == '|') { cells.Add(cur.ToString().Trim()); cur.Clear(); continue; }
            cur.Append(s[k]);
        }
        cells.Add(cur.ToString().Trim());
        return cells;
    }

    // ---- 块渲染 ----------------------------------------------------------

    private static void AddHeading(FlowDocument doc, Blk b, ReadingMode mode, double sc)
    {
        int lv = Math.Clamp(b.Level, 1, 6);
        double size = lv switch { 1 => Px(25, sc), 2 => Px(21, sc), 3 => Px(18, sc), 4 => Px(16.5, sc), _ => Px(15.5, sc) };
        var p = new Paragraph
        {
            FontSize = size,
            LineHeight = size * (lv <= 2 ? 1.5 : 1.6),
            FontWeight = lv <= 3 ? FontWeights.Bold : FontWeights.SemiBold,
            Foreground = lv <= 3 ? HeadInk : QuoteInk,
            Margin = new Thickness(0, (lv == 1 ? 24 : lv == 2 ? 19 : 14) * sc, 0, (lv <= 2 ? 9 : 7) * sc),
            KeepWithNext = true,
        };
        if (lv <= 2)
        {
            p.BorderBrush = LineBrush;
            p.BorderThickness = new Thickness(0, 0, 0, 1);
            p.Padding = new Thickness(0, 0, 0, 5 * sc);
        }
        if (b.Marker.Length > 0)
        {
            if (mode == ReadingMode.Raw) p.Inlines.Add(new Run(b.Marker + " ") { Foreground = BodyInk, FontWeight = p.FontWeight });
            else if (mode == ReadingMode.Dim) p.Inlines.Add(new Run(b.Marker + " ") { Foreground = MarkupBrush, FontWeight = FontWeights.Normal });
        }
        if (b.Lines.Count > 0) Fill(p, b.Lines[0], mode);
        doc.Blocks.Add(p);
    }

    private static void AddParagraph(FlowDocument doc, Blk b, ReadingMode mode, double sc)
    {
        var p = NewBody(sc, new Thickness(0, 0, 0, 11 * sc));
        Fill(p, Join(b.Lines), mode);
        doc.Blocks.Add(p);
    }

    private static Paragraph NewBody(double sc, Thickness margin) => new()
    {
        FontSize = Px(15.5, sc),
        LineHeight = Px(15.5, sc) * 1.85,
        Margin = margin,
    };

    private static void AddQuote(FlowDocument doc, Blk b, ReadingMode mode, double sc)
    {
        var p = NewBody(sc, new Thickness(b.Level * 12 * sc, 0, 0, 13 * sc));
        p.Foreground = QuoteInk;
        p.Background = QuoteBack;
        p.BorderBrush = QuoteBar;
        p.BorderThickness = new Thickness(3, 0, 0, 0);
        p.Padding = new Thickness(12, 5, 10, 5);
        Fill(p, Join(b.Lines), mode);
        doc.Blocks.Add(p);
    }

    private static void AddItem(FlowDocument doc, Blk b, ReadingMode mode, double sc)
    {
        var ordered = b.Marker.Length > 0 && char.IsDigit(b.Marker[0]);
        var p = NewBody(sc, new Thickness((16 + b.Level * 20) * sc, 0, 0, (b.Loose ? 10 : 4) * sc));
        p.TextIndent = -(ordered ? 20 : 16) * sc;

        if (b.Marker.Length > 0 && !ordered)
        {
            // 隐藏档 + 任务项：方框本身就是记号，不再叠一个圆点
            if (mode == ReadingMode.Hidden && b.Task.Length == 0) p.Inlines.Add(new Run(b.Level == 0 ? "•  " : "–  ") { Foreground = MarkupBrush });
            else if (mode != ReadingMode.Hidden) p.Inlines.Add(new Run(b.Marker) { Foreground = mode == ReadingMode.Raw ? BodyInk : MarkupBrush });
        }
        else if (b.Marker.Length > 0)
        {
            p.Inlines.Add(new Run(b.Marker) { Foreground = MarkupBrush, FontWeight = FontWeights.SemiBold });
        }
        if (b.Task.Length > 0)
        {
            bool done = b.Task[1] != ' ';
            if (mode == ReadingMode.Raw) p.Inlines.Add(new Run(b.Task) { Foreground = BodyInk });
            else if (mode == ReadingMode.Hidden) p.Inlines.Add(new Run(done ? "☑  " : "☐  ")
            { Foreground = done ? LinkInk : MarkupBrush, FontWeight = done ? FontWeights.SemiBold : FontWeights.Normal });
            else p.Inlines.Add(new Run(b.Task) { Foreground = MarkupBrush });
        }
        if (b.Lines.Count > 0) Fill(p, Join(b.Lines), mode);
        doc.Blocks.Add(p);
    }

    private static void AddCode(FlowDocument doc, Blk b, double sc)
    {
        double size = Px(13.5, sc);
        if (b.Lang.Length > 0)
        {
            var tag = new Paragraph(new Run(b.Lang) { Foreground = MetaBrush, FontFamily = Mono, FontSize = Px(11, sc) })
            {
                Margin = new Thickness(0, 8 * sc, 0, 0),
                Padding = new Thickness(12, 3, 12, 1),
                Background = CodeBack,
                LineHeight = Px(11, sc) * 1.4,
            };
            doc.Blocks.Add(tag);
        }
        for (int k = 0; k < b.Lines.Count; k++)
        {
            var p = new Paragraph
            {
                FontFamily = Mono,
                FontSize = size,
                LineHeight = size * 1.55,
                Foreground = CodeInk,
                Background = CodeBack,
                Margin = new Thickness(0),
                Padding = new Thickness(12, k == 0 && b.Lang.Length == 0 ? 8 : 1, 12, k == b.Lines.Count - 1 ? 8 : 1),
            };
            p.Inlines.Add(new Run(b.Lines[k].Length == 0 ? " " : b.Lines[k]));
            doc.Blocks.Add(p);
        }
        var gap = new Paragraph(new Run(" ")) { FontSize = Px(6, sc), LineHeight = Px(10, sc), Margin = new Thickness(0, 0, 0, 8 * sc) };
        doc.Blocks.Add(gap);
    }

    private static void AddRule(FlowDocument doc, double sc)
    {
        doc.Blocks.Add(new Paragraph(new Run(" "))
        {
            FontSize = Px(2, sc),
            LineHeight = 2,
            Margin = new Thickness(0, 12 * sc, 0, 14 * sc),
            BorderBrush = LineBrush,
            BorderThickness = new Thickness(0, 0, 0, 1),
        });
    }

    private static void AddMeta(FlowDocument doc, Blk b, double sc)
    {
        for (int k = 0; k < b.Lines.Count; k++)
        {
            var p = new Paragraph
            {
                FontFamily = Mono,
                FontSize = Px(12, sc),
                LineHeight = Px(12, sc) * 1.5,
                Foreground = MetaBrush,
                Background = QuoteBack,
                Margin = new Thickness(0),
                Padding = new Thickness(12, k == 0 ? 7 : 0, 12, k == b.Lines.Count - 1 ? 7 : 0),
            };
            p.Inlines.Add(new Run(b.Lines[k].Length == 0 ? " " : b.Lines[k]) { Foreground = MarkupBrush });
            doc.Blocks.Add(p);
        }
        doc.Blocks.Add(new Paragraph(new Run(" ")) { FontSize = Px(6, sc), LineHeight = Px(10, sc), Margin = new Thickness(0, 0, 0, 10 * sc) });
    }

    private static void AddTable(FlowDocument doc, Blk b, ReadingMode mode, double sc)
    {
        var header = Cells(b.Header[0]);
        int cols = Math.Max(header.Count, b.Aligns.Count);
        foreach (var r in b.Rows) cols = Math.Max(cols, Cells(r).Count);
        if (cols < 1) return;

        if (mode != ReadingMode.Hidden)
        {
            // 原样 / 弱化：竖线属于要留下的阅读字符，就按行上屏，只是不硬折行
            foreach (var src in new[] { b.Header[0] }.Concat(b.Rows))
            {
                var p = new Paragraph
                {
                    FontFamily = Mono,
                    FontSize = Px(13, sc),
                    LineHeight = Px(13, sc) * 1.5,
                    Margin = new Thickness(0, 0, 0, 1),
                    Padding = new Thickness(3, 1, 3, 1),
                };
                foreach (var cell in src.Split('|'))
                {
                    if (p.Inlines.Count > 0) p.Inlines.Add(new Run("|") { Foreground = mode == ReadingMode.Raw ? BodyInk : MarkupBrush });
                    p.Inlines.Add(new Run(cell));
                }
                doc.Blocks.Add(p);
            }
            doc.Blocks.Add(new Paragraph(new Run(" ")) { FontSize = Px(6, sc), LineHeight = Px(10, sc), Margin = new Thickness(0, 0, 0, 8 * sc) });
            return;
        }

        var table = new Table
        {
            CellSpacing = 0,
            Margin = new Thickness(0, 2, 0, 14 * sc),
        };
        var grp = new TableRowGroup();
        table.RowGroups.Add(grp);
        for (int c = 0; c < cols; c++) table.Columns.Add(new TableColumn { Width = new GridLength(1, GridUnitType.Star) });

        void AddRow(List<string> cells, bool isHead, int index)
        {
            var row = new TableRow();
            for (int c = 0; c < cols; c++)
            {
                var txt = c < cells.Count ? cells[c] : "";
                var para = new Paragraph
                {
                    FontSize = Px(13.5, sc),
                    LineHeight = Px(13.5, sc) * 1.5,
                    Margin = new Thickness(0),
                    FontWeight = isHead ? FontWeights.SemiBold : FontWeights.Normal,
                    Foreground = isHead ? HeadInk : BodyInk,
                    TextAlignment = AlignOf(c, b.Aligns),
                };
                if (txt.Length > 0) Fill(para, txt, mode);
                row.Cells.Add(new TableCell(para)
                {
                    Background = isHead ? CellHead : index % 2 == 1 ? CellAlt : Brushes.White,
                    BorderBrush = CellLine,
                    BorderThickness = new Thickness(0, 0, 1, 1),
                    Padding = new Thickness(9, 5, 9, 5),
                });
            }
            grp.Rows.Add(row);
        }

        AddRow(header, true, 0);
        for (int r = 0; r < b.Rows.Count; r++) AddRow(Cells(b.Rows[r]), false, r);
        doc.Blocks.Add(table);
    }

    private static TextAlignment AlignOf(int col, List<string> aligns)
    {
        if (col >= aligns.Count) return TextAlignment.Left;
        var a = aligns[col];
        if (a.StartsWith(":") && a.EndsWith(":")) return TextAlignment.Center;
        return a.EndsWith(":") ? TextAlignment.Right : TextAlignment.Left;
    }

    // ---- 段内合并（只在屏幕上把硬换行接回去，文件里的换行一个字节都没动） ----

    private static string Join(List<string> lines)
    {
        if (lines.Count == 0) return "";
        if (lines.Count == 1) return lines[0];
        var sb = new StringBuilder();
        for (int k = 0; k < lines.Count; k++)
        {
            var cur = lines[k].Trim();
            if (k > 0)
            {
                var prev = lines[k - 1];
                var p = prev.TrimEnd();
                bool hard = prev.EndsWith("  ") || prev.EndsWith("\\") || p.EndsWith("<br>") || p.EndsWith("<br/>");
                if (hard) sb.Append('\n');
                else if (p.Length > 0 && cur.Length > 0 && IsCjk(p[^1]) && IsCjk(cur[0])) { /* 汉字之间不补空格 */ }
                else sb.Append(' ');
            }
            sb.Append(cur);
        }
        return sb.ToString();
    }

    /// <summary>中日韩与全角字符：这些字符之间接行时不补空格，否则中文里会多出缝来。</summary>
    private static bool IsCjk(char c) =>
        c is >= '　' and <= '〿'     // CJK 标点
           or >= 'ぁ' and <= 'ヿ'          // 假名
           or >= '㐀' and <= '䶿'          // 汉字扩展 A
           or >= '一' and <= '鿿'          // 基本汉字
           or >= '가' and <= 'ힿ'          // 谚文
           or >= '豈' and <= '﫿'          // 兼容汉字
           or >= '！' and <= '｠';         // 全角形式

    // ---- 行内渲染 --------------------------------------------------------

    private static readonly Regex InlineRx = new(
        @"(?<img>!\[[^\]\n]*\]\([^)\n]*\))" +
        @"|(?<link>\[(?<lt>[^\]\n]*)\]\((?<lu>[^)\n]*)\))" +
        @"|(?<code>`+[^`\n]+`+)" +
        @"|(?<bs>\*\*[^*\n]+\*\*)" +
        @"|(?<bu>__[^_\n]+__)" +
        @"|(?<es>\*[^*\n]+\*)" +
        @"|(?<eu>(?<![\w])_[^_\n]+_(?![\w]))" +
        @"|(?<del>~~[^~\n]+~~)" +
        @"|(?<html><[A-Za-z/][^>\n]*>)",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    /// <summary>把一段 Markdown 文本填进段落。'\n' 是硬换行，落成 LineBreak。</summary>
    private static void Fill(Paragraph p, string s, ReadingMode mode)
    {
        var hidden = mode == ReadingMode.Hidden;
        int pos = 0;

        void Mark(string t)
        {
            if (t.Length > 0) p.Inlines.Add(new Run(t) { Foreground = MarkupBrush });
        }
        void Styled(string t, Action<Run>? style)
        {
            if (t.Length == 0) return;
            var parts = t.Split('\n');
            for (int k = 0; k < parts.Length; k++)
            {
                if (k > 0) p.Inlines.Add(new LineBreak());
                if (parts[k].Length == 0) continue;
                var r = new Run(parts[k]);
                style?.Invoke(r);
                p.Inlines.Add(r);
            }
        }

        foreach (Match m in InlineRx.Matches(s))
        {
            if (m.Index > pos) Styled(s[pos..m.Index], null);
            pos = m.Index + m.Length;
            var g = m.Groups;

            void Pair(string whole, string open, string inner, string close, Action<Run> style)
            {
                if (mode == ReadingMode.Raw) { Styled(whole, null); return; }
                if (!hidden) Mark(open);
                Styled(inner, style);
                if (!hidden) Mark(close);
            }

            if (g["code"].Success)
            {
                var t = g["code"].Value;
                var ticks = t.TakeWhile(c => c == '`').Count();
                Pair(t, t[..ticks], t[ticks..^ticks], t[^ticks..], r =>
                {
                    r.FontFamily = Mono;
                    r.FontSize = p.FontSize * 0.9;
                    if (hidden) { r.Foreground = CodeInk; r.Background = CodeBack; }
                });
            }
            else if (g["bs"].Success || g["bu"].Success)
            {
                var t = (g["bs"].Success ? g["bs"] : g["bu"]).Value;
                Pair(t, t[..2], t[2..^2], t[^2..], r => r.FontWeight = FontWeights.SemiBold);
            }
            else if (g["es"].Success || g["eu"].Success)
            {
                var t = (g["es"].Success ? g["es"] : g["eu"]).Value;
                Pair(t, t[..1], t[1..^1], t[^1..], r => r.FontStyle = FontStyles.Italic);
            }
            else if (g["del"].Success)
            {
                var t = g["del"].Value;
                Pair(t, t[..2], t[2..^2], t[^2..], r => r.TextDecorations = TextDecorations.Strikethrough);
            }
            else if (g["link"].Success)
            {
                var txt = g["lt"].Value;
                var url = g["lu"].Value;
                if (mode == ReadingMode.Raw) { Styled(m.Value, null); continue; }
                if (!hidden) Mark("[" + txt + "](");
                var run = new Run(txt.Length == 0 ? url : txt) { Foreground = LinkInk, TextDecorations = TextDecorations.Underline };
                ToolTipService.SetToolTip(run, url);
                AutomationProperties.SetHelpText(run, url);   // 链接目标没丢，只是挪到提示里
                p.Inlines.Add(run);
                if (!hidden) Mark(url.Length <= 26 ? url + ")" : url[..23] + "…)");
            }
            else if (g["img"].Success)
            {
                if (hidden)
                {
                    var alt = Regex.Match(g["img"].Value, @"!\[([^\]]*)\]").Groups[1].Value;
                    p.Inlines.Add(new Run($"［图：{(string.IsNullOrWhiteSpace(alt) ? "无替代文字" : alt)}］") { Foreground = MetaBrush });
                }
                else Mark(g["img"].Value);
            }
            else if (g["html"].Success)
            {
                if (!hidden) Mark(m.Value);
            }
        }
        if (pos < s.Length) Styled(s[pos..], null);
    }
}
