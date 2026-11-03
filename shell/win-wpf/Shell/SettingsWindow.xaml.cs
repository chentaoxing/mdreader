using System.Diagnostics;
using System.IO;
using System.Windows;
using Microsoft.Win32;

namespace MdFileAssistant.Shell;

/// <summary>
/// 设置窗口。它改的是「保存去哪、要不要弹、多久存一次」这些规矩，
/// 不碰文件内容，也不写注册表。取消=什么都不改；保存=立刻生效并落 settings.json。
/// </summary>
public partial class SettingsWindow : Window
{
    private readonly Settings _s;

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
    }

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
