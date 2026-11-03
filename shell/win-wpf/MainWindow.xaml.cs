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
    private byte[] _original = Array.Empty<byte>();     // 磁盘上最后一次读到/写回的字节，用于"外部改过就拒写"
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
        UpdateSrcMeta();
        if (_transcoded)
            LblStatus.Text = $"这份文件是 {dec.Label}。显示与优化都在内存里的 UTF-8 副本上做，原文件一个字节都不动；另存出去的会是 UTF-8 新文件。"
                + (dec.Label.Contains("GB")
                    ? "（无 BOM 文件是按内容猜的编码，若显示乱码请人工确认原编码）"
                    : "");

        Run();
        if (_editOn) SetEdit(false);      // 换文件就回到只读：编辑不跨文件，也不被记住
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
        // 只在内容真的不一样时才回写左栏：编辑模式下用户的光标不能因为切阅读模式/字号被抹掉
        if (ViewSrc.Text != _srcText) ViewSrc.Text = _srcText;
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
            InitialDirectory = _settings.SaveAsDirOr(_path),
            FileName = name,
        };
        if (dlg.ShowDialog(this) != true) return;

        if (string.Equals(Path.GetFullPath(dlg.FileName), Path.GetFullPath(_path), StringComparison.OrdinalIgnoreCase))
        {
            MessageBox.Show(this, "「另存为优化版」不写原文件——写原文件是「保存回源文件」的活。请换个文件名。",
                "MD文档阅读助手");
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

    // ---- 编辑模式 --------------------------------------------------------
    //
    // 默认关闭，而且每次启动、每次换文件都回到关闭：这个软件对外的承诺是"不改你的文件"，
    // 编辑是那条承诺的例外，所以它不能被记住、不能被默认打开。
    // 开之前先弹 EditWarningWindow（大面积、大字体），只有点最后一颗按钮才真的开。

    private bool _editOn;
    private bool _dirty;
    private bool _busy;                       // 弹窗开着时 WPF 仍在泵消息，连点会叠出第二个、第三个模态框
    private DispatcherTimer? _editDebounce;
    private DispatcherTimer? _autoArc;        // 定时把没保存的内容写一份到缓存目录

    private void OnEdit(object s, RoutedEventArgs e)
    {
        if (_busy) return;
        if (_editOn)
        {
            if (_dirty)
            {
                MessageBoxResult ask;
                _busy = true;
                try
                {
                    ask = MessageBox.Show(this,
                        "左栏有还没保存的改动。关掉编辑会丢弃这些改动（源文件不会被改动）。\n\n" +
                        "丢弃前会先把现在的内容写一份到缓存目录，找得回来。",
                        "MD文档阅读助手", MessageBoxButton.OKCancel, MessageBoxImage.Warning);
                }
                finally { _busy = false; }
                if (ask != MessageBoxResult.OK) return;
                var (lostPath, _) = WriteCacheCopy();
                OpenFile(_path);
                SetEdit(false);
                LblStatus.Text = "编辑已关闭，没保存的改动已丢弃；源文件还是磁盘上那份。" +
                                 (lostPath == null ? "" : " 丢弃前的内容留在 " + Path.GetFileName(lostPath));
                return;
            }
            SetEdit(false);
            LblStatus.Text = "编辑已关闭，左栏回到只读。源文件一个字节都没动。";
            return;
        }

        if (string.IsNullOrWhiteSpace(_path) || _original.Length == 0)
        {
            LblStatus.Text = "先打开一份 .md 再谈编辑——现在没有文件可改。";
            return;
        }

        // 那屏大字警告默认必弹；设置里关掉就直接进编辑（表头仍会变红，按钮仍在）
        if (_settings.WarnBeforeEdit)
        {
            var dlg = new EditWarningWindow { Owner = this };
            _busy = true;
            try { dlg.ShowDialog(); } finally { _busy = false; }
            if (!dlg.Confirmed)
            {
                LblStatus.Text = "没开编辑。左栏还是只读，源文件一个字节都不会动。";
                return;
            }
        }
        SetEdit(true);
    }

    private void SetEdit(bool on)
    {
        _editOn = on;
        _dirty = false;
        ViewSrc.IsReadOnly = !on;
        ViewSrc.IsUndoEnabled = on;                       // 只读时没必要留撤销栈，编辑时必须有
        BtnEdit.Content = on ? "编辑：开（点这里关闭）" : "编辑：关";
        BtnSaveSrc.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        BtnSaveSrc.IsEnabled = false;
        BtnSaveSrc.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        BtnSaveSrc.IsEnabled = false;
        RefreshEditUi();
        if (!on) { _editDebounce?.Stop(); _autoArc?.Stop(); return; }

        _editDebounce ??= new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(350) };
        _editDebounce.Tick -= OnEditDebounce;
        _editDebounce.Tick += OnEditDebounce;
        _editDebounce.Stop();

        if (_settings.AutoArchive)
        {
            _autoArc ??= new DispatcherTimer();
            _autoArc.Tick -= OnAutoArchive;
            _autoArc.Tick += OnAutoArchive;
            _autoArc.Interval = TimeSpan.FromMinutes(Math.Clamp(_settings.AutoArchiveMinutes, 1, 240));
            _autoArc.Start();
        }
        else _autoArc?.Stop();

        LblStatus.Text = (_settings.OverwritesSource
                ? "编辑已开启：改动点「保存回源文件」才写回这个文件。"
                : "编辑已开启：改动点「另存为新文件」写到原件旁边，原件不动。")
            + (_settings.AutoArchive
                ? $" 开着编辑时每 {_settings.AutoArchiveMinutes} 分钟另存一份到缓存目录。" : "");
        ViewSrc.Focus();
    }

    private void OnSrcTextChanged(object s, TextChangedEventArgs e)
    {
        if (!_editOn || ViewSrc.Text == _srcText) return;
        _srcText = ViewSrc.Text;
        _dirty = true;
        BtnSaveSrc.IsEnabled = true;
        _editDebounce?.Stop();
        _editDebounce?.Start();
    }

    /// <summary>停手 350ms 后，用改过的文本重跑一次核心，让右栏读到的就是左栏现在的样子。</summary>
    private void OnEditDebounce(object? s, EventArgs e)
    {
        _editDebounce?.Stop();
        if (!_editOn) return;
        _coreBytes = new UTF8Encoding(false).GetBytes(_srcText);
        Run();
        // 直接引按钮自己的文案，免得改了按钮忘了改状态条
        if (_dirty) LblStatus.Text = "● 有未保存的改动 —— 点「" + BtnSaveSrc.Content + "」才会写盘。";
    }

    private void OnSaveSrc(object s, RoutedEventArgs e)
    {
        if (_busy || !_editOn || string.IsNullOrWhiteSpace(_path)) return;
        if (!_dirty) { LblStatus.Text = "左栏没有改动，不用保存。"; return; }

        // 默认走这条：原件不动，改动写到原件旁边一个新文件
        if (!_settings.OverwritesSource) { SaveAsNewFile(); return; }

        // 落盘前先重读一次：文件在我们打开之后被别的程序改过，就不写，也不猜哪份对
        byte[] now;
        try { now = File.ReadAllBytes(_path); }
        catch (Exception ex) { LblStatus.Text = "✗ 保存前重读失败：" + ex.Message + " —— 什么都没写。"; return; }
        if (!now.AsSpan().SequenceEqual(_original))
        {
            _busy = true;
            try
            {
                MessageBox.Show(this,
                    "这份文件在你打开之后被别的程序改过了（重读出来的字节和打开时不一样）。\n\n" +
                    "我不覆盖，也不合并。请自己决定：\n" +
                    "  · 想留磁盘上这份 —— 重新打开一次文件，你刚改的会丢；\n" +
                    "  · 想留你改的这份 —— 用「另存为优化版…」存成新文件，再自己替换。",
                    "MD文档阅读助手", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
            finally { _busy = false; }
            LblStatus.Text = "✗ 磁盘上的文件已经变了，拒绝写回（源文件未被改动）。";
            return;
        }

        var bytes = TextDecode.Encode(_srcText, _encLabel, _original, out var encErr);
        if (bytes == null)
        {
            _busy = true;
            try
            {
                MessageBox.Show(this, encErr ?? "写不回原编码。", "MD文档阅读助手",
                    MessageBoxButton.OK, MessageBoxImage.Warning);
            }
            finally { _busy = false; }
            LblStatus.Text = "✗ 写不回原编码，拒绝保存（源文件未被改动）。";
            return;
        }

        // 保命副本：只在没有的时候建，里面永远是改动前的原件。建不出来就不写——
        // 弹窗里答应了有这份备份，不能说话不算。
        var bak = _path + ".bak";
        try { if (!File.Exists(bak)) File.WriteAllBytes(bak, _original); }
        catch (Exception ex)
        {
            LblStatus.Text = "✗ 保命副本 .bak 建不出来（" + ex.Message + "），所以这次不写。";
            return;
        }

        var dir = Path.GetDirectoryName(_path) ?? ".";
        var tmp = Path.Combine(dir, "." + Path.GetFileName(_path) + ".mdfile-tmp");
        string how;
        try
        {
            File.WriteAllBytes(tmp, bytes);
            try { File.Replace(tmp, _path, null); how = "原子替换"; }
            catch { File.Move(tmp, _path, true); how = "覆盖写入"; }
        }
        catch (Exception ex)
        {
            try { if (File.Exists(tmp)) File.Delete(tmp); } catch { /* 清不掉临时文件，不掩盖真正的写失败 */ }
            LblStatus.Text = "✗ 写入失败：" + ex.Message + " —— 源文件保持原样。";
            return;
        }

        var oldLen = _original.Length;
        _original = bytes;
        _dirty = false;
        BtnSaveSrc.IsEnabled = false;
        UpdateSrcMeta();
        LblStatus.Text = "✓ 已写回 " + Path.GetFileName(_path) + "（" + how + "，" +
                         oldLen.ToString("N0") + " → " + bytes.Length.ToString("N0") + " 字节 · " + _encLabel +
                         " · 改动前的原件在 " + Path.GetFileName(bak) + "）";
    }

    /// <summary>左栏表头那行「多少字节 · 什么编码 · 什么换行」，读进来和写回去都靠它说话。</summary>
    private void UpdateSrcMeta()
    {
        var nl = _srcText.Contains("\r\n") ? "CRLF" : _srcText.Contains('\r') ? "CR" : "LF";
        LblSrcMeta.Text = _original.Length.ToString("N0") + " 字节 · " + _encLabel + " · " + nl +
                          (_original.StartsWith(new byte[] { 0xEF, 0xBB, 0xBF }) ? " · 有 BOM" : "");
    }

    /// <summary>把「保存去哪」这件事在界面上说清楚：按钮文案 + 左栏表头，都跟着设置走。</summary>
    private void RefreshEditUi()
    {
        BtnSaveSrc.Content = _settings.OverwritesSource ? "保存回源文件" : "另存为新文件";
        LblSrcHead.Text = _editOn
            ? (_settings.OverwritesSource ? "源文件（编辑中 · 保存会写回这个文件）"
                                          : "源文件（编辑中 · 保存写到同目录的新文件）")
            : "源文件（只读，永不写回）";
        LblSrcHead.Foreground = _editOn ? Theme.ThemeManager.Brush("WarnInk")
                                        : Theme.ThemeManager.Brush("TextPrimary");
    }

    private static string SaveStamp() => DateTime.Now.ToString("yyyyMMdd-HHmmss");

    /// <summary>
    /// 往缓存目录写一份当前内容。缓存是「还没决定怎么落盘」的东西，所以：
    /// 写不进原编码就退成 UTF-8（文件名后缀会标出来），它不是用来替换原件的，是用来找回来的。
    /// </summary>
    private (string? Path, string? Error) WriteCacheCopy()
    {
        if (string.IsNullOrWhiteSpace(_path)) return (null, "没有打开的文件，缓存没地方写");
        try
        {
            var dir = _settings.CacheDirOrDefault;
            Directory.CreateDirectory(dir);
            var stem = Path.GetFileNameWithoutExtension(_path);
            var full = Path.Combine(dir, stem + "." + SaveStamp() + ".md");
            var bytes = TextDecode.Encode(_srcText, _encLabel, _original, out var encErr);
            if (bytes == null)
            {
                bytes = new UTF8Encoding(false).GetBytes(_srcText);
                full = Path.Combine(dir, stem + "." + SaveStamp() + ".退回UTF-8.md");
                LblStatus.Text = "⚠ 缓存这份按 " + _encLabel + " 写不下（" + encErr + "），已改存成 UTF-8。";
            }
            File.WriteAllBytes(full, bytes);
            PruneCache(dir, stem);
            return (full, null);
        }
        catch (Exception ex) { return (null, ex.Message); }
    }

    /// <summary>只清理「这个源文件」的缓存，按最近修改时间留下最新几份。</summary>
    private void PruneCache(string dir, string stem)
    {
        if (_settings.AutoArchiveKeep <= 0) return;
        try
        {
            var stale = Directory.GetFiles(dir, stem + ".*.md*")
                .Where(f => !string.Equals(f, _path, StringComparison.OrdinalIgnoreCase))
                .OrderByDescending(f => File.GetLastWriteTimeUtc(f))
                .Skip(_settings.AutoArchiveKeep);
            foreach (var f in stale) { try { File.Delete(f); } catch { /* 删不掉就留着，不值得为这个弹窗 */ } }
        }
        catch { /* 列目录失败同样不拦用户干活 */ }
    }

    private void OnAutoArchive(object? s, EventArgs e)
    {
        if (!_editOn || !_dirty) return;              // 没改动就不制造一堆一样的缓存
        var (p, err) = WriteCacheCopy();
        LblStatus.Text = p != null
            ? "◦ 自动存档已写入缓存：" + Path.GetFileName(p) + " · 左栏的改动还没落盘"
            : "✗ 自动存档没写进去（" + err + "）——改动只在屏幕上，还没保存。";
    }

    /// <summary>默认的保存：写到原件旁边一个新文件，原件一个字节不动。</summary>
    private void SaveAsNewFile()
    {
        var dir = Path.GetDirectoryName(_path) ?? "";
        if (string.IsNullOrEmpty(dir)) { LblStatus.Text = "✗ 这个文件没有所在目录，写不出新文件。"; return; }

        var bytes = TextDecode.Encode(_srcText, _encLabel, _original, out var encErr);
        if (bytes == null)
        {
            _busy = true;
            try { MessageBox.Show(this, encErr ?? "写不回原编码。", "MD文档阅读助手",
                    MessageBoxButton.OK, MessageBoxImage.Warning); }
            finally { _busy = false; }
            LblStatus.Text = "✗ 写不回原编码，没有落盘（原件本来也没动）。";
            return;
        }

        var stem = Path.GetFileNameWithoutExtension(_path);
        var target = Path.Combine(dir, stem + ".改于" + SaveStamp() + ".md");
        for (int i = 2; File.Exists(target) && i < 100; i++)          // 同一秒里连点两次别互相盖掉
            target = Path.Combine(dir, $"{stem}.改于{SaveStamp()}-{i}.md");
        try { File.WriteAllBytes(target, bytes); }
        catch (Exception ex) { LblStatus.Text = "✗ 写不出去：" + ex.Message + " —— 原件未动。"; return; }

        _dirty = false;
        BtnSaveSrc.IsEnabled = false;
        LblStatus.Text = "✓ 已另存为 " + Path.GetFileName(target) + "（" + bytes.Length.ToString("N0") +
                         " 字节 · " + _encLabel + " · 原件 " + Path.GetFileName(_path) + " 未动）";
    }

    private void OnSettings(object s, RoutedEventArgs e)
    {
        if (_busy) return;
        var dlg = new SettingsWindow(_settings) { Owner = this };
        _busy = true;
        bool? ok;
        try { ok = dlg.ShowDialog(); } finally { _busy = false; }
        if (ok != true) return;

        _settings.Save();
        RefreshEditUi();
        if (_editOn)
        {
            if (_settings.AutoArchive)
            {
                _autoArc ??= new DispatcherTimer();
                _autoArc.Tick -= OnAutoArchive;
                _autoArc.Tick += OnAutoArchive;
                _autoArc.Interval = TimeSpan.FromMinutes(Math.Clamp(_settings.AutoArchiveMinutes, 1, 240));
                _autoArc.Start();
            }
            else _autoArc?.Stop();
        }
        LblStatus.Text = "✓ 设置已保存：保存目标「" + _settings.EditSaveTarget + "」 · 自动存档 " +
                         (_settings.AutoArchive ? "每 " + _settings.AutoArchiveMinutes + " 分钟" : "关") +
                         " · 编辑前弹确认 " + (_settings.WarnBeforeEdit ? "开" : "关") +
                         " · 关闭时询问 " + (_settings.AskOnClose ? "开" : "关") + "。";
    }

    /// <summary>
    /// 关软件时如果有没保存的改动：先把当前内容写进缓存目录，再问一句。
    /// 顺序很重要——先写后问，所以「不保存」也不会真丢，缓存那份还在。
    /// </summary>
    protected override void OnClosing(CancelEventArgs e)
    {
        if (_editOn && _dirty)
        {
            var (cache, cacheErr) = WriteCacheCopy();
            if (_settings.AskOnClose)
            {
                _busy = true;
                MessageBoxResult r;
                try
                {
                    r = MessageBox.Show(this,
                        "左栏还有没保存的改动。\n\n" +
                        "· 「是」= 保存后再关（按设置：写回源文件）\n" +
                        "· 「否」= 直接关。改动已经先写进缓存目录了，" +
                        (cache != null ? "在 " + Path.GetFileName(cache) : "但这次没写进去：" + cacheErr) + "\n" +
                        "· 「取消」= 不关，回去继续改",
                        "MD文档阅读助手", MessageBoxButton.YesNoCancel, MessageBoxImage.Warning);
                }
                finally { _busy = false; }

                if (r == MessageBoxResult.Cancel) { e.Cancel = true; return; }
                if (r == MessageBoxResult.Yes)
                {
                    e.Cancel = true;
                    OnSaveSrc(this, new RoutedEventArgs());
                    if (!_dirty) Dispatcher.BeginInvoke(new Action(Close));   // 真写成了才关，没写成留在原地
                    return;
                }
            }
        }
        _autoArc?.Stop();
        base.OnClosing(e);
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
