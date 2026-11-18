using System.Text.Json.Serialization;
using System.Windows;
using System.Windows.Media;

namespace MdFileAssistant.Reading;

/// <summary>
/// 版式（Typography）：阅读视图「怎么排」的一组参数——字体、字号、行距、段距、缩进、对齐、
/// 行宽、页边距、标题缩放、列表缩进、表格密度。
///
/// 它是**纯显示层**：只影响屏幕上 FlowDocument 的样子，绝不写回 .md 文件，也不进正文指纹。
/// 与 <c>crates/mdcore/src/presets.rs</c> 里的「预设 / Preset」是两码事——那是调**重排规则**
/// （空行、并列分行），这里调**观感**。两个词、两个下拉、两套命名，永不混用。
///
/// 所有默认值都精确复刻 v0.2.3 的现有观感：默认 = 零视觉回归。
/// </summary>
public sealed class Typography
{
    public const string DefaultBodyFont = "Microsoft YaHei UI, Segoe UI";
    public const string DefaultMonoFont = "Consolas, Cascadia Mono, Courier New";

    public string BodyFont { get; set; } = DefaultBodyFont;
    public string MonoFont { get; set; } = DefaultMonoFont;

    public double BodySize { get; set; } = 15.5;            // 正文字号（pt，仍再乘整体缩放因子 sc）
    public double LineHeightFactor { get; set; } = 1.85;    // 正文行距倍数
    public double ParaGap { get; set; } = 11;               // 段落间距（pt）
    public double FirstLineIndent { get; set; } = 0;        // 首行缩进，单位「字」（em）；0=不缩进
    public string Align { get; set; } = "left";             // left | justify
    public double Measure { get; set; } = 880;              // 行宽上限（MaxPageWidth 基准像素）
    public double PageMarginScale { get; set; } = 1.0;      // 页边距整体倍率（1=现有 22/18/22/30）
    public double HeadingScale { get; set; } = 1.0;         // 各级标题字号整体倍率
    public double ListIndentScale { get; set; } = 1.0;      // 列表缩进整体倍率
    public double TableDensity { get; set; } = 1.0;         // 表格内边距/字号密度倍率

    [JsonIgnore] public FontFamily BodyFamily => new(Normalize(BodyFont, DefaultBodyFont));
    [JsonIgnore] public FontFamily MonoFamily => new(Normalize(MonoFont, DefaultMonoFont));
    [JsonIgnore] public TextAlignment AlignEnum => Align == "justify" ? TextAlignment.Justify : TextAlignment.Left;

    /// <summary>被人手改坏或留空时退回默认字体栈，别让渲染拿到空字体。</summary>
    private static string Normalize(string? s, string fallback) =>
        string.IsNullOrWhiteSpace(s) ? fallback : s.Trim();

    public Typography Clone() => new()
    {
        BodyFont = BodyFont, MonoFont = MonoFont, BodySize = BodySize,
        LineHeightFactor = LineHeightFactor, ParaGap = ParaGap, FirstLineIndent = FirstLineIndent,
        Align = Align, Measure = Measure, PageMarginScale = PageMarginScale,
        HeadingScale = HeadingScale, ListIndentScale = ListIndentScale, TableDensity = TableDensity,
    };

    /// <summary>设置文件被人手改坏时，把越界数值夹回合理区间；认不出的对齐退回左对齐。</summary>
    public void Sanitize()
    {
        BodySize = Clamp(BodySize, 9, 36);
        LineHeightFactor = Clamp(LineHeightFactor, 1.0, 3.0);
        ParaGap = Clamp(ParaGap, 0, 40);
        FirstLineIndent = Clamp(FirstLineIndent, 0, 4);
        Measure = Clamp(Measure, 360, 2000);
        PageMarginScale = Clamp(PageMarginScale, 0.4, 2.5);
        HeadingScale = Clamp(HeadingScale, 0.6, 2.0);
        ListIndentScale = Clamp(ListIndentScale, 0.5, 2.0);
        TableDensity = Clamp(TableDensity, 0.6, 2.0);
        Align = Align == "justify" ? "justify" : "left";
    }

    private static double Clamp(double v, double lo, double hi) =>
        double.IsFinite(v) ? Math.Clamp(v, lo, hi) : lo;
}
