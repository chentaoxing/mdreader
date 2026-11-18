using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Microsoft.Win32;
using MdFileAssistant.Reading;

namespace MdFileAssistant.Shell;

/// <summary>
/// 设置窗口。它改的是「保存去哪、要不要弹、多久存一次」这些规矩，以及「版式（怎么排）」——
/// 版式是纯显示层：不碰文件内容，也不写注册表。取消=什么都不改；保存=立刻生效并落 settings.json。
/// </summary>
public partial class SettingsWindow : Window
{
    private readonly Settings _s;
    private bool _ready;   // InitializeComponent 期间滑块会先触发 ValueChanged，那时标签字段还没建好，必须挡住

    public SettingsWindow(Settings current)
    {
        _s = current;
        InitializeComponent();
        RSrcNew.IsChecked = !_s.OverwritesSource;
        RSrcOver.IsChecked = _s.OverwritesSource;
        TxtSaveAsDir.Text = _s.SaveAsDir;
        ChkAutoArc.IsChecked = _s.AutoArchive;
        TxtArcMinutes.Text = _s.AutoArchiveMinutes.ToString();
        TxtArcKeep.Text = _s.AutoArchiveKeep.ToString();
        TxtArcDir.Text = _s.CacheDirOrDefault;
        ChkWarnEdit.IsChecked = _s.WarnBeforeEdit;
        ChkAskClose.IsChecked = _s.AskOnClose;
        LblWhere.Text = "设置存 " +
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "MdFileAssistant", "settings.json") + "，纯文本，可直接删。";
        UpdateArcNote();
        InitTypographyTab();
        _ready = true;
    }

    // ---- 版式 tab --------------------------------------------------------

    private static readonly string[] PreferredBodyFonts =
        { "Microsoft YaHei UI, Segoe UI", "微软雅黑", "宋体", "黑体", "楷体", "等线", "Segoe UI" };
    private static readonly string[] PreferredMonoFonts =
        { "Consolas, Cascadia Mono, Courier New", "Consolas", "Cascadia Mono", "Courier New", "宋体" };

    private void InitTypographyTab()
    {
        FillFonts(CmbBodyFont, _s.Typography.BodyFont, PreferredBodyFonts);
        FillFonts(CmbMonoFont, _s.Typography.MonoFont, PreferredMonoFonts);
        LoadTypographyIntoControls(_s.Typography);
    }

    /// <summary>常用字体兜底列表 + 可编辑（用户可自行输入本机已装字体名）。
    /// P1 不枚举全部系统字体，避免依赖不稳定 API；后续若要全列表再补。</summary>
    private static void FillFonts(ComboBox cmb, string current, string[] preferred)
    {
        cmb.ItemsSource = preferred;
        cmb.Text = string.IsNullOrWhiteSpace(current) ? preferred[0] : current;
    }

    private void LoadTypographyIntoControls(Typography t)
    {
        CmbBodyFont.Text = t.BodyFont;
        CmbMonoFont.Text = t.MonoFont;
        CmbAlign.SelectedIndex = t.Align == "justify" ? 1 : 0;
        SldSize.Value = t.BodySize;
        SldLine.Value = t.LineHeightFactor;
        SldParaGap.Value = t.ParaGap;
        SldIndent.Value = t.FirstLineIndent;
        SldMeasure.Value = t.Measure;
        SldMargin.Value = t.PageMarginScale;
        SldHeading.Value = t.HeadingScale;
        SldListIndent.Value = t.ListIndentScale;
        SldTable.Value = t.TableDensity;
        RefreshTypeLabels();
    }

    private void OnTypeSlider(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (!_ready) return;   // 构造期滑块赋值触发的 ValueChanged 一律忽略，避免碰未创建的标签控件
        RefreshTypeLabels();
    }

    private void RefreshTypeLabels()
    {
        LblSize.Text = $"{SldSize.Value:F1}pt";
        LblLine.Text = $"{SldLine.Value:F2}";
        LblParaGap.Text = $"{SldParaGap.Value:F0}pt";
        LblIndent.Text = $"{SldIndent.Value:F1}字";
        LblMeasure.Text = $"{SldMeasure.Value:F0}";
        LblMargin.Text = $"{SldMargin.Value:F2}×";
        LblHeading.Text = $"{SldHeading.Value:F2}×";
        LblListIndent.Text = $"{SldListIndent.Value:F2}×";
        LblTable.Text = $"{SldTable.Value:F2}×";
    }

    private void OnResetType(object s, RoutedEventArgs e) => LoadTypographyIntoControls(new Typography());

    private static string PickFont(ComboBox cmb, string fallback) =>
        string.IsNullOrWhiteSpace(cmb.Text) ? fallback : cmb.Text.Trim();

    private void ReadTypographyInto(Settings s)
    {
        var t = s.Typography;
        t.BodyFont = PickFont(CmbBodyFont, Typography.DefaultBodyFont);
        t.MonoFont = PickFont(CmbMonoFont, Typography.DefaultMonoFont);
        t.Align = CmbAlign.SelectedIndex == 1 ? "justify" : "left";
        t.BodySize = SldSize.Value;
        t.LineHeightFactor = SldLine.Value;
        t.ParaGap = SldParaGap.Value;
        t.FirstLineIndent = SldIndent.Value;
        t.Measure = SldMeasure.Value;
        t.PageMarginScale = SldMargin.Value;
        t.HeadingScale = SldHeading.Value;
        t.ListIndentScale = SldListIndent.Value;
        t.TableDensity = SldTable.Value;
        t.Sanitize();
    }

    // ---- 其余 tab（原有逻辑）--------------------------------------------

    private void UpdateArcNote()
    {
        LblArcNote.Text = ChkAutoArc.IsChecked == true
            ? "缓存目录不是废纸篓：它存的是「你改了但还没决定怎么落盘」的每一份。关掉自动存档不会删已有文件。"
            : "关掉之后不再定时写缓存；已经存下的文件保留，要清就自己去那个目录删。";
        PanelArc.Opacity = ChkAutoArc.IsChecked == true ? 1.0 : 0.45;
        PanelArc.IsEnabled = ChkAutoArc.IsChecked == true;
        TxtArcDir.IsEnabled = ChkAutoArc.IsChecked == true;
        BtnPickArcDir.IsEnabled = ChkAutoArc.IsChecked == true;
    }

    private void OnAutoArcChanged(object s, RoutedEventArgs e) => UpdateArcNote();

    /// <summary>WPF 的目录选择框（.NET 8 起才有 OpenFolderDialog，WinForms 那个不在这个项目里）。</summary>
    private static string? PickFolder(Window owner, string title)
    {
        var d = new OpenFolderDialog { Title = title, Multiselect = false };
        if (d.ShowDialog(owner) != true) return null;
        var p = d.FolderName;
        return string.IsNullOrWhiteSpace(p) ? null : Path.TrimEndingDirectorySeparator(p);
    }

    private void OnPickSaveDir(object s, RoutedEventArgs e)
    {
        var p = PickFolder(this, "选「另存为」的默认目录");
        if (p != null) TxtSaveAsDir.Text = p;
    }

    private void OnClearSaveDir(object s, RoutedEventArgs e) => TxtSaveAsDir.Text = "";

    private void OnPickArcDir(object s, RoutedEventArgs e)
    {
        var p = PickFolder(this, "选自动存档的缓存目录");
        if (p != null) TxtArcDir.Text = p;
    }

    private void OnOpenArcDir(object s, RoutedEventArgs e)
    {
        var dir = string.IsNullOrWhiteSpace(TxtArcDir.Text) ? Settings.DefaultCacheDir : TxtArcDir.Text;
        try
        {
            Directory.CreateDirectory(dir);
            Process.Start(new ProcessStartInfo("explorer.exe", "\"" + dir + "\"") { UseShellExecute = true });
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, "打不开那个目录：" + ex.Message, "设置");
        }
    }

    private static int ParseInt(string raw, int fallback, int min, int max)
        => int.TryParse((raw ?? "").Trim(), out var v) && v >= min && v <= max ? v : fallback;

    private void OnOk(object s, RoutedEventArgs e)
    {
        _s.EditSaveTarget = RSrcOver.IsChecked == true ? Settings.SaveOverwriteSource : Settings.SaveNewToSameDir;
        _s.SaveAsDir = TxtSaveAsDir.Text.Trim();
        _s.AutoArchive = ChkAutoArc.IsChecked == true;
        _s.AutoArchiveMinutes = ParseInt(TxtArcMinutes.Text, 2, 1, 240);
        _s.AutoArchiveKeep = ParseInt(TxtArcKeep.Text, 40, 0, 5000);
        var dir = TxtArcDir.Text.Trim();
        _s.AutoArchiveDir = dir == Settings.DefaultCacheDir ? "" : dir;
        _s.WarnBeforeEdit = ChkWarnEdit.IsChecked == true;
        _s.AskOnClose = ChkAskClose.IsChecked == true;
        ReadTypographyInto(_s);

        if (!string.IsNullOrWhiteSpace(_s.SaveAsDir) && !Directory.Exists(_s.SaveAsDir))
        {
            MessageBox.Show(this, "「另存为」目录不存在，已改回每次从源文件所在目录开始。", "设置");
            _s.SaveAsDir = "";
            TxtSaveAsDir.Text = "";
        }
        _s.Save();
        DialogResult = true;
        Close();
    }

    private void OnCancel(object s, RoutedEventArgs e)
    {
        DialogResult = false;
        Close();
    }
}
