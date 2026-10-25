using System.ComponentModel;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using Microsoft.Win32;
using MdFileAssistant.Native;
using MdFileAssistant.Reading;
using MdFileAssistant.Shell;

namespace MdFileAssistant;

public partial class MainWindow : Window
{
    private byte[] _original = Array.Empty<byte>();     // 磁盘上的原始字节，永不写回
    private byte[] _coreBytes = Array.Empty<byte>();    // 交给核心的 UTF-8 副本（同编码时就是原件）
    private bool _transcoded;                           // 是否发生过内存内转码
    private string _encLabel = "UTF-8";
    private string _srcText = "";
    private string _path = "";
    private OptimizeResult _result = new();
    private List<RuleRow> _rows = new();
    private bool _ready;
    private Settings _settings = new();

    /// <summary>字号档位。只影响屏幕上的版式，不写回文件。</summary>
    private static readonly (string Name, double Scale)[] TypeScales =
    {
        ("小", 0.88), ("标准", 1.0), ("大", 1.16), ("特大", 1.34),
    };

    public MainWindow()
    {
        InitializeComponent();
        _settings = Settings.Load();
        ApplyModeRadio(_settings.Mode);
        ApplyThemeRadio(_settings.Theme);
        CmbType.ItemsSource = TypeScales.Select(t => t.Name).ToList();
        var tw = Array.FindIndex(TypeScales, t => t.Name == _settings.Type);
        CmbType.SelectedIndex = tw >= 0 ? tw : 1;
        ApplyView(_settings.View == "专心读");
        ChkAuto.IsChecked = _settings.AutoCheck;
        _ready = true;
        // 主题变了要重画阅读视图：FlowDocument 的颜色是渲染当时定下来的，不会自己跟着变
        Theme.ThemeManager.Reapplied += OnThemeReapplied;

        if (_settings.AutoCheck)
        {
            // 启动 3 秒后才查，且只在"确实有新版"时出声——不打扰正在读的东西
            var t = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
            t.Tick += (_, _) => { t.Stop(); _ = CheckUpdateAsync(silent: true); };
            t.Start();
        }

        if (!MdCore.Available)
        {
            LblStatus.Text = "找不到 mdcore_ffi.dll —— 排版判定全部来自它，缺了就没有任何功能可做。请把它放在程序同目录。";
            BtnOpen.IsEnabled = false;
            return;
        }
        var rules = MdCore.Rules(out var presets);
        CmbPreset.ItemsSource = presets;
        var want = presets.IndexOf(_settings.Preset);
        CmbPreset.SelectedIndex = want >= 0 ? want : (presets.Count > 0 ? 0 : -1);
        LblStatus.Text = $"引擎 {MdCore.EngineVersion} · 规则 {rules.Count} 条 · 预设 {presets.Count} 种。左栏是文件本身，右栏是给人读的版式。";

        if (App.StartupFile is { Length: > 0 } f) OpenFile(f);
    }

    private void ApplyModeRadio(string mode)
    {
        switch (mode)
        {
            case nameof(ReadingMode.Raw): MRaw.IsChecked = true; break;
            case nameof(ReadingMode.Hidden): MHide.IsChecked = true; break;
            default: MDim.IsChecked = true; break;
        }
    }

    private void ApplyThemeRadio(string theme)
    {
        switch (theme)
        {
            case "浅色": TLight.IsChecked = true; break;
            case "深色": TDark.IsChecked = true; break;
            default: TFollow.IsChecked = true; break;
        }
    }

    private string ThemeChoice => TDark.IsChecked == true ? "深色"
                              : TLight.IsChecked == true ? "浅色" : "跟随系统";

    private void OnThemeChanged(object s, RoutedEventArgs e)
    {
        // 注意：XAML 解析到 IsChecked="True" 时就会触发这里，那一刻 TLight/TDark 还没被创建，
        // 读它们会 NullReference（第一次真启动就是这么崩的）。_ready 之前一律不动作：
        // 主题在 App.OnStartup 里已经按设置应用过一次了。
        if (!_ready) return;
        _settings.Theme = ThemeChoice;
        _settings.Save();
        Theme.ThemeManager.Apply(ThemeChoice);
    }

    private void OnThemeReapplied()
    {
        if (!_ready) return;
        Show();
        if (_original.Length > 0) SayScore();
    }

    private string Preset => CmbPreset.SelectedItem as string ?? "通用文章";
    private ReadingMode Mode => MHide.IsChecked == true ? ReadingMode.Hidden
                              : MRaw.IsChecked == true ? ReadingMode.Raw : ReadingMode.Dim;

    private void OnOpen(object s, RoutedEventArgs e)
    {
        var dlg = new OpenFileDialog { Filter = "Markdown|*.md;*.markdown|所有文件|*.*", Multiselect = false };
        if (dlg.ShowDialog(this) == true) OpenFile(dlg.FileName);
    }

    private void OnDragOver(object s, DragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop) ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
    }

    private void OnDrop(object s, DragEventArgs e)
    {
        // v0.1.4：DLL 缺失的降级态只禁了按钮，拖放仍会走进 OpenFile→Run→MdCore，
        // DllNotFoundException 没人接就是崩溃。这里先拦一道。
        if (!MdCore.Available)
        {
            LblStatus.Text = "找不到 mdcore_ffi.dll —— 排版判定全部来自它，缺了就没有任何功能可做。请把它放在程序同目录。";
            return;
        }
        if (e.Data.GetData(DataFormats.FileDrop) is string[] { Length: > 0 } files) OpenFile(files[0]);
    }

    private void OpenFile(string path)
    {
        byte[] bytes;
        try { bytes = File.ReadAllBytes(path); }
        catch (Exception ex) { MessageBox.Show(this, "读不了这个文件：" + ex.Message, "MD文档阅读助手"); return; }

        _path = path;
        _original = bytes;
        LblFile.Text = Path.GetFileName(path);

        var dec = TextDecode.Decode(bytes);
        if (dec == null)
        {
            _coreBytes = Array.Empty<byte>();
            _transcoded = false;
            _encLabel = "认不出";
            _srcText = "";
            ViewSrc.Text = "";
            ViewOut.Document = ReadingRenderer.RenderReading("", Mode, Scale);
            GridRules.ItemsSource = null;
            BtnSave.IsEnabled = false;
            LblSrcMeta.Text = $"{bytes.Length:N0} 字节 · 编码未知";
            LblStatus.Text = "✗ 既不是 UTF-8，也不是 GBK / GB18030 / UTF-16 —— 认不出的编码我不猜，文件也没动。";
            return;
        }

        _coreBytes = dec.Utf8;
        _transcoded = dec.Transcoded;
        _encLabel = dec.Label;
        _srcText = dec.Text;
        var nl = dec.Text.Contains("\r\n") ? "CRLF" : dec.Text.Contains('\r') ? "CR" : "LF";
        LblSrcMeta.Text = $"{bytes.Length:N0} 字节 · {dec.Label} · {nl}" +
                          (bytes.StartsWith(new byte[] { 0xEF, 0xBB, 0xBF }) ? " · 有 BOM" : "");
        if (_transcoded)
            LblStatus.Text = $"这份文件是 {dec.Label}。显示与优化都在内存里的 UTF-8 副本上做，原文件一个字节都不动；另存出去的会是 UTF-8 新文件。"
                + (dec.Label.Contains("GB")
                    ? "（无 BOM 文件是按内容猜的编码，若显示乱码请人工确认原编码）"
                    : "");

        Run();
    }

    private double Scale
    {
        get
        {
            var idx = CmbType.SelectedIndex;
            return idx >= 0 && idx < TypeScales.Length ? TypeScales[idx].Scale : 1.0;
        }
    }

    private void Run()
    {
        if (_coreBytes.Length == 0)
        {
            // v0.1.4：空文件是一个独立状态。旧实现直接 return，上一份文件的优化结果与
            // 可点的「另存为」原样残留——打开 A 再打开空文件 B 后点另存，会写出
            // 「B_阅读优化版.md」但内容是 A 的优化结果。这里清干净。
            _rows = new List<RuleRow>();
            GridRules.ItemsSource = null;
            BtnSave.IsEnabled = false;
            ViewSrc.Text = _srcText;
            ViewOut.Document = ReadingRenderer.RenderReading("", Mode, Scale);
            LblOutMeta.Text = "";
            return;
        }
        _result = MdCore.Optimize(_coreBytes, Preset, "preserve");
        if (!_result.Ok)
        {
            LblStatus.Text = $"核心返回 {_result.Status}（{MdCore.StatusText(_result.Status)}）：{_result.Error}";
            ViewSrc.Text = _srcText;
            ViewOut.Document = ReadingRenderer.RenderReading("", Mode, Scale);
            GridRules.ItemsSource = null;
            BtnSave.IsEnabled = false;
            return;
        }
        _rows = BuildRows(_result);
        GridRules.ItemsSource = _rows;
        BtnSave.IsEnabled = true;
        Show();
        SayScore();
    }

    private List<RuleRow> BuildRows(OptimizeResult r)
    {
        var rows = r.Applied.Select(a => new RuleRow(a.RuleId, a.RuleName, a.Scope, a.Edits, a.Reason, "")
        { OnChanged = () => RebuildFromRows() }).ToList();
        foreach (var j in r.Rejected)
            rows.Add(new RuleRow(j.RuleId, j.RuleName, "", 0, "", $"{j.Reason}（核心已拒绝，不会落盘）")
            { Accept = false, Locked = true, OnChanged = () => RebuildFromRows() });
        return rows;
    }

    /// <summary>逐条接受 / 拒绝后重建：走核心的 rebuild，界面不参与任何排版判断。</summary>
    private void RebuildFromRows()
    {
        if (_coreBytes.Length == 0 || _rows.Count == 0) return;
        var decisions = _rows.Where(x => !x.Locked).Select(x => (x.RuleId, x.Accept)).ToList();
        var r = MdCore.Rebuild(_coreBytes, Preset, decisions);
        if (!r.Ok)
        {
            LblStatus.Text = $"重建返回 {r.Status}（{MdCore.StatusText(r.Status)}）：{r.Error ?? MdCore.LastError()}";
            return;
        }
        // 与「原始正文」的校验和比（rebuild 只回重建后那一个）；比字符串，不比 JSON 里的数字
        if (r.ChecksumOptimized.Length == 0 ||
            (_result.ChecksumOriginal.Length > 0 && r.ChecksumOptimized != _result.ChecksumOriginal))
        {
            LblStatus.Text = $"✗ 重建后正文校验和与原文不一致（{Short(r.ChecksumOptimized)} ≠ {Short(_result.ChecksumOriginal)}），不采用";
            return;
        }
        _result.Body = r.Body;
        _result.Rendered = r.Rendered;
        _result.ContentChanges = r.ContentChanges;
        _result.ChecksumOptimized = r.ChecksumOptimized.Length > 0 ? r.ChecksumOptimized : _result.ChecksumOptimized;
        _result.StructuralChanges = _rows.Where(x => x.Accept && !x.Locked).Sum(x => x.Edits);
        Show(quiet: false);
    }

    private void Show(bool quiet = false)
    {
        ViewSrc.Text = _srcText;                                   // 左栏：文件原样，不随阅读模式变
        ViewOut.Document = ReadingRenderer.RenderReading(_result.Body, Mode, Scale);
        var kept = _rows.Count(x => x.Accept && !x.Locked);
        LblOutMeta.Text = $"保留 {kept}/{_rows.Count} 条规则 · {_result.StructuralChanges} 处结构变化";
        LblStatus.Text = _result.ContentUntouched
            ? $"✓ 正文内容变化 0 · 结构变化 {_result.StructuralChanges} 处 · 校验和 {Short(_result.ChecksumOriginal)} 前后一致 · 收敛轮次 {_result.Passes} · 阅读视图的版式只在屏幕上，不写回文件"
              + (_transcoded ? $" · 源文件是 {_encLabel}，现在读的是内存里的 UTF-8 副本" : "")
            : $"✗ 正文内容变化 {_result.ContentChanges} 处 —— 不允许保存";
    }

    private static string Short(string s) => s.Length <= 10 ? s : s[..6] + "…" + s[^3..];

    private void SayScore()
    {
        PanelScores.Children.Clear();
        var a = MdCore.Analyze(_coreBytes, Preset);
        if (!a.Ok) return;
        void Row(string label, int before, int after)
        {
            var g = new Grid { Margin = new Thickness(0, 3, 0, 3) };
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(88) });
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(74) });
            var t = new TextBlock { Text = label, FontSize = 12.5, VerticalAlignment = VerticalAlignment.Center };
            var bar = new Border
            {
                Height = 9,
                Margin = new Thickness(6, 0, 6, 0),
                CornerRadius = new CornerRadius(4),
                Background = Theme.ThemeManager.Brush("BarTrack"),
                Child = new Border
                {
                    Width = 0,
                    CornerRadius = new CornerRadius(4),
                    Background = after >= before ? Theme.ThemeManager.Brush("TextInk") : Theme.ThemeManager.Brush("StatusBad"),
                    HorizontalAlignment = HorizontalAlignment.Left,
                },
            };
            ((Border)bar.Child).Width = Math.Max(2, after * 1.6);
            var v = new TextBlock
            {
                Text = $"{before} → {after}",
                FontSize = 12.5,
                Foreground = after >= before ? Theme.ThemeManager.Brush("TextMuted") : Theme.ThemeManager.Brush("StatusBad"),
                VerticalAlignment = VerticalAlignment.Center,
            };
            Grid.SetColumn(t, 0); Grid.SetColumn(bar, 1); Grid.SetColumn(v, 2);
            g.Children.Add(t); g.Children.Add(bar); g.Children.Add(v);
            PanelScores.Children.Add(g);
        }
        Row("综合", a.Score.Overall, _result.After.Overall);
        Row("标题结构", a.Score.Heading, _result.After.Heading);
        Row("段落结构", a.Score.Paragraph, _result.After.Paragraph);
        Row("列表结构", a.Score.List, _result.After.List);
        Row("空白分布", a.Score.Whitespace, _result.After.Whitespace);
        Row("信息密度", a.Score.Density, _result.After.Density);

        ListFindings.Items.Clear();
        foreach (var f in a.Findings.Take(60))
            ListFindings.Items.Add($"第 {f.Line + 1} 行 · {f.Dimension} · {f.Detail}");
        if (a.Findings.Count == 0) ListFindings.Items.Add("没有发现问题。");
    }

    private void OnModeChanged(object s, RoutedEventArgs e)
    {
        if (!_ready) return;
        _settings.Mode = Mode.ToString();
        _settings.Save();
        Show();
    }

    private void OnTypeChanged(object s, SelectionChangedEventArgs e)
    {
        if (CmbType.SelectedIndex >= 0 && CmbType.SelectedIndex < TypeScales.Length)
            _settings.Type = TypeScales[CmbType.SelectedIndex].Name;
        if (!_ready) return;
        _settings.Save();
        Show();
    }

    private void OnViewChanged(object s, RoutedEventArgs e)
    {
        if (!_ready) return;
        var focus = VFocus.IsChecked == true;
        _settings.View = focus ? "专心读" : "对比源文件";
        _settings.Save();
        ApplyView(focus);
    }

    /// <summary>专心读＝把文件栏收起、给正文一条合适的行长；不改任何字节，只是窗口怎么摆。</summary>
    private void ApplyView(bool focus)
    {
        if (focus)
        {
            if (VFocus.IsChecked != true) VFocus.IsChecked = true;
            ColSrc.Width = new GridLength(0);
            ColGap.Width = new GridLength(0);
            PaneSrc.Visibility = Visibility.Collapsed;
            Splitter.Visibility = Visibility.Collapsed;
            ViewOut.MaxWidth = 920;
            ViewOut.HorizontalAlignment = HorizontalAlignment.Center;
            LblOutTitle.Text = "阅读视图";
        }
        else
        {
            if (VCompare.IsChecked != true) VCompare.IsChecked = true;
            ColSrc.Width = new GridLength(0.85, GridUnitType.Star);
            ColGap.Width = new GridLength(12);
            PaneSrc.Visibility = Visibility.Visible;
            Splitter.Visibility = Visibility.Visible;
            ViewOut.MaxWidth = double.PositiveInfinity;
            ViewOut.HorizontalAlignment = HorizontalAlignment.Stretch;
            LblOutTitle.Text = "优化后的读法";
        }
    }

    private void OnPresetChanged(object s, SelectionChangedEventArgs e)
    {
        if (CmbPreset.SelectedItem is string ps) { _settings.Preset = ps; _settings.Save(); }
        if (!_ready || _original.Length == 0) return;
        Run();
    }

    private void OnSave(object s, RoutedEventArgs e)
    {
        if (_coreBytes.Length == 0) return; // v0.1.4：没有已加载的正文可保存（空文件态）
        if (!_result.Ok || !_result.ContentUntouched)
        {
            MessageBox.Show(this, "正文内容发生了变化，这个结果不允许保存。", "MD文档阅读助手");
            return;
        }
        var name = string.IsNullOrWhiteSpace(_result.OutputName)
            ? Path.GetFileNameWithoutExtension(_path) + "_阅读优化版.md"
            : _result.OutputName;
        var dlg = new SaveFileDialog
        {
            Title = "另存为优化版",
            Filter = "Markdown|*.md",
            InitialDirectory = Path.GetDirectoryName(_path),
            FileName = name,
        };
        if (dlg.ShowDialog(this) != true) return;

        if (string.Equals(Path.GetFullPath(dlg.FileName), Path.GetFullPath(_path), StringComparison.OrdinalIgnoreCase))
        {
            MessageBox.Show(this, "这个工具永不写入原文件——请换个文件名。", "MD文档阅读助手");
            return;
        }
        try
        {
            File.WriteAllBytes(dlg.FileName, _result.Rendered);
            LblStatus.Text = $"✓ 已写出 {Path.GetFileName(dlg.FileName)}（原文件未动，正文变化 0"
                + (_transcoded ? $"；源文件是 {_encLabel}，这份写的是 UTF-8" : "") + "）";
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, "写入失败：" + ex.Message, "MD文档阅读助手");
        }
    }

    private void OnAssoc(object s, RoutedEventArgs e)
    {
        if (FileAssociation.IsRegistered())
        {
            var ask = MessageBox.Show(this,
                "已注册。要撤销吗？\n\n撤销只是把本程序从 .md 的「打开方式」列表里移除，不会改你的默认应用。",
                "MD文档阅读助手", MessageBoxButton.YesNo, MessageBoxImage.Question);
            if (ask != MessageBoxResult.Yes) return;
            LblStatus.Text = FileAssociation.Unregister().Replace("\n", "  ");
            return;
        }
        var report = FileAssociation.Register();
        // v0.1.4：Register 提前失败时报告里可能一行 ✓/✗ 都没有，
        // 旧的 .First(...) 直接抛 InvalidOperationException。
        LblStatus.Text = report.Split('\n').FirstOrDefault(l => l.StartsWith('✓') || l.StartsWith('✗'))
                         ?? report.Split('\n').FirstOrDefault(l => l.Trim().Length > 0)
                         ?? "注册流程没有返回结果";
        MessageBox.Show(this, report + "\n\n（Windows 的「默认应用」只能由你亲手在系统设置里点，程序不能代劳。）",
            "MD文档阅读助手", MessageBoxButton.OK, MessageBoxImage.Information);
        FileAssociation.OpenSystemDefaults();
    }

    // ---- 自更新 ----------------------------------------------------------

    private void OnAutoChanged(object s, RoutedEventArgs e)
    {
        if (!_ready) return;
        _settings.AutoCheck = ChkAuto.IsChecked == true;
        _settings.Save();
        LblStatus.Text = _settings.AutoCheck
            ? "已开启启动时检查更新。这是本程序唯一的出网行为，只查版本号、不上传任何内容。"
            : "已关闭启动时检查更新。以后要更新，点右上角「检查更新」。";
    }

    private void OnCheckUpdate(object s, RoutedEventArgs e) => _ = CheckUpdateAsync(silent: false);

    private bool _checking;

    private async Task CheckUpdateAsync(bool silent)
    {
        if (_checking) return;
        _checking = true;
        BtnUpdate.IsEnabled = false;
        var before = LblStatus.Text;
        if (!silent) LblStatus.Text = "正在检查更新（只访问 GitHub 的发布页，不上传任何内容）…";
        Updater.CheckResult r;
        try { r = await Updater.CheckAsync(); }
        finally { BtnUpdate.IsEnabled = true; _checking = false; }

        if (!r.Ok)
        {
            if (silent) LblStatus.Text = before;
            else MessageBox.Show(this, "检查更新没成功：" + r.Error, "MD文档阅读助手");
            return;
        }
        if (!r.Available)
        {
            if (!silent) LblStatus.Text = $"已经是最新版 v{r.Current}（检查走的是{r.ProxyNote}）";
            return;
        }

        if (silent)
        {
            // 静默档不打扰：把话留在按钮和状态条上，他自己决定什么时候点
            BtnUpdate.Content = $"有新版本 v{r.Latest}";
            LblStatus.Text = $"发现新版本 v{r.Latest}（当前 v{r.Current}）· 点右上角「检查更新」下载 · 检查走的是{r.ProxyNote}";
            return;
        }

        var ask = MessageBox.Show(this,
            $"发现新版本 v{r.Latest}（当前 v{r.Current}）。\n更新包：{r.AssetName ?? "（没找到 zip，只能打开发布页）"}\n\n" +
            "要现在下载并安装吗？\n下载完会关掉本程序、覆盖安装目录、然后自动重新打开；\n" +
            "这一步只覆盖程序自己的文件，不碰你的 .md。",
            "MD文档阅读助手", MessageBoxButton.YesNo, MessageBoxImage.Question);
        if (ask != MessageBoxResult.Yes) return;

        var prog = new Progress<string>(t => LblStatus.Text = "下载更新包 " + t);
        string zip;
        try
        {
            zip = await Updater.DownloadAndResolveAsync(r, prog);
        }
        catch (Exception e)
        {
            LblStatus.Text = "✗ 更新包没拿到：" + Updater.Friendly(e);
            MessageBox.Show(this, "更新失败：\n" + Updater.Friendly(e) +
                "\n\n也可以直接到发布页自己下载：" + $"https://github.com/{Updater.Repo}/releases/latest",
                "MD文档阅读助手");
            return;
        }
        LblStatus.Text = "✓ 更新包已下载并校验，正在交给安装脚本…";
        // v0.1.4：helper.bat 写盘（TEMP 被清）或启动（杀软拦截）抛出的异常曾挂在
        // 被 `_ =` 丢弃的 Task 上，状态条永远停在「正在交给安装脚本」。
        try
        {
            SelfReplace.InstallAndRestart(zip);
        }
        catch (Exception ex)
        {
            LblStatus.Text = "✗ 交给安装脚本失败：" + ex.Message;
            MessageBox.Show(this,
                "更新没有安装成功（" + ex.Message + "）。\n\n更新包还在：" + zip +
                "\n可以手动解压覆盖程序目录。", "MD文档阅读助手");
            return;
        }
        Application.Current.Shutdown();     // 退出后 helper.bat 才继续覆盖
    }
}

/// <summary>改动清单里的一行。勾选变化即触发核心重建，界面自己不做任何排版计算。</summary>
public sealed class RuleRow : INotifyPropertyChanged
{
    public RuleRow(string id, string name, string scope, int edits, string reason, string rejected)
    {
        RuleId = id; RuleName = name; Scope = scope; Edits = edits; Reason = reason; Rejected = rejected;
    }
    public string RuleId { get; }
    public string RuleName { get; }
    public string Scope { get; }
    public int Edits { get; }
    public string Reason { get; }
    public string Rejected { get; }
    public bool Locked { get; init; }
    public Action? OnChanged;

    private bool _accept = true;
    public bool Accept
    {
        get => _accept;
        set
        {
            if (_accept == value || Locked) return;
            _accept = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Accept)));
            OnChanged?.Invoke();
        }
    }
    public event PropertyChangedEventHandler? PropertyChanged;
}
