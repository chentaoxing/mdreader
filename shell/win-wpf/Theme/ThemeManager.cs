using System.Collections.Generic;
using Microsoft.Win32;
using System.Windows;
using System.Windows.Media;

namespace MdFileAssistant.Theme;

/// <summary>
/// 明暗主题。两个事实先说清楚：
/// 1. 颜色只有这一处定义（<see cref="Light"/> / <see cref="Dark"/>），界面 XAML 与阅读视图都从这里取，
///    所以不会出现"某块忘改还是白的"。
/// 2. 「跟随系统」是默认档：读 Windows 的 AppsUseLightTheme，并订阅 SystemEvents，
///    你在系统设置里切夜间模式，窗口当场跟着变，不需要重启。
///    手动选浅色/深色则记住选择，不再跟随。
/// </summary>
public static class ThemeManager
{
    public const string Follow = "跟随系统";
    public const string LightName = "浅色";
    public const string DarkName = "深色";

    /// <summary>语义色名 → 浅色值。命名按"用在哪"，不按"长什么样"。</summary>
    private static readonly Dictionary<string, Color> Light = new()
    {
        ["WindowBg"] = C(0xFB, 0xFB, 0xFD),
        ["PanelBg"] = C(0xFF, 0xFF, 0xFF),
        ["PanelLine"] = C(0xE2, 0xE6, 0xEC),
        ["HeaderBg"] = C(0xF7, 0xF9, 0xFB),
        ["HeaderOutBg"] = C(0xF1, 0xF7, 0xF4),
        ["TextPrimary"] = C(0x22, 0x29, 0x2F),
        ["TextMuted"] = C(0x8A, 0x90, 0x99),
        ["TextInk"] = C(0x0B, 0x3B, 0x5C),
        ["Markup"] = C(0xAE, 0xB4, 0xBD),
        ["Meta"] = C(0x8A, 0x90, 0x99),
        ["CodeInk"] = C(0x14, 0x4A, 0x6E),
        ["CodeBg"] = C(0xF2, 0xF5, 0xF8),
        ["QuoteInk"] = C(0x45, 0x52, 0x5C),
        ["QuoteBg"] = C(0xF7, 0xF9, 0xFB),
        ["QuoteBar"] = C(0xC3, 0xD2, 0xE0),
        ["Rule"] = C(0xD8, 0xDE, 0xE6),
        ["TableHead"] = C(0xEE, 0xF3, 0xF7),
        ["TableRowAlt"] = C(0xFA, 0xFB, 0xFD),
        ["TableBorder"] = C(0xDD, 0xE3, 0xE9),
        ["LinkInk"] = C(0x14, 0x53, 0x2D),
        ["StatusFg"] = C(0x3B, 0x40, 0x48),
        ["StatusBad"] = C(0xB3, 0x26, 0x1E),
        ["Selection"] = C(0xBF, 0xD8, 0xEA),
        ["FieldBg"] = C(0xFF, 0xFF, 0xFF),
        ["FieldText"] = C(0x22, 0x29, 0x2F),
        ["BarTrack"] = C(0xEE, 0xF1, 0xF5),
    };

    private static readonly Dictionary<string, Color> Dark = new()
    {
        ["WindowBg"] = C(0x17, 0x19, 0x1C),
        ["PanelBg"] = C(0x1E, 0x21, 0x26),
        ["PanelLine"] = C(0x2E, 0x32, 0x38),
        ["HeaderBg"] = C(0x23, 0x26, 0x2B),
        ["HeaderOutBg"] = C(0x1F, 0x2A, 0x25),
        ["TextPrimary"] = C(0xDD, 0xE2, 0xE8),
        ["TextMuted"] = C(0x8B, 0x92, 0x9B),
        ["TextInk"] = C(0x7F, 0xB6, 0xE0),
        ["Markup"] = C(0x5A, 0x61, 0x6B),
        ["Meta"] = C(0x8B, 0x92, 0x9B),
        ["CodeInk"] = C(0x9F, 0xD0, 0xF5),
        ["CodeBg"] = C(0x26, 0x2A, 0x31),
        ["QuoteInk"] = C(0xB9, 0xC2, 0xCC),
        ["QuoteBg"] = C(0x23, 0x26, 0x2B),
        ["QuoteBar"] = C(0x3D, 0x46, 0x50),
        ["Rule"] = C(0x34, 0x38, 0x3F),
        ["TableHead"] = C(0x26, 0x2B, 0x33),
        ["TableRowAlt"] = C(0x21, 0x25, 0x2A),
        ["TableBorder"] = C(0x35, 0x3A, 0x42),
        ["LinkInk"] = C(0x7C, 0xC7, 0x96),
        ["StatusFg"] = C(0xC6, 0xCD, 0xD5),
        ["StatusBad"] = C(0xF0, 0x80, 0x7A),
        ["Selection"] = C(0x2F, 0x4A, 0x63),
        ["FieldBg"] = C(0x23, 0x26, 0x2B),
        ["FieldText"] = C(0xDD, 0xE2, 0xE8),
        ["BarTrack"] = C(0x2A, 0x2E, 0x35),
    };

    private static Color C(byte r, byte g, byte b) => Color.FromRgb(r, g, b);

    private static readonly Dictionary<string, SolidColorBrush> Cache = new();

    public static string Mode { get; private set; } = Follow;
    public static bool IsDark { get; private set; }

    /// <summary>主题换好后要重画阅读视图（FlowDocument 里的颜色是渲染时定的，不会自己跟着变）。</summary>
    public static event Action? Reapplied;

    public static void Apply(string mode)
    {
        Mode = mode switch { LightName or DarkName => mode, _ => Follow };
        var dark = Mode == DarkName || (Mode == Follow && !SystemPrefersLight());
        IsDark = dark;
        var palette = dark ? Dark : Light;
        var app = Application.Current;
        if (app == null) return;
        foreach (var (key, color) in palette)
        {
            var brush = new SolidColorBrush(color);
            brush.Freeze();
            Cache[key] = brush;
            app.Resources[key] = brush;
        }
        Reapplied?.Invoke();
    }

    /// <summary>阅读视图取色：渲染当时取，切主题后重画就换新色。</summary>
    public static SolidColorBrush Brush(string key)
    {
        if (Cache.TryGetValue(key, out var b)) return b;
        var palette = IsDark ? Dark : Light;
        var nb = new SolidColorBrush(palette[key]);
        nb.Freeze();
        Cache[key] = nb;
        return nb;
    }

    public static Color ColorOf(string key) => (IsDark ? Dark : Light)[key];

    /// <summary>Windows 的「应用默认颜色模式」：1=浅色，0=深色。读不到按浅色。</summary>
    public static bool SystemPrefersLight()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            var v = key?.GetValue("AppsUseLightTheme");
            return v is not int i || i != 0;
        }
        catch (Exception) { return true; }
    }

    private static bool hooked;

    /// <summary>跟随系统：订阅 UserPreferenceChanged，系统一切换就重新应用。</summary>
    public static void HookSystemChanges()
    {
        if (hooked) return;
        hooked = true;
        SystemEvents.UserPreferenceChanged += OnUserPreference;
        Application.Current.Exit += (_, _) => SystemEvents.UserPreferenceChanged -= OnUserPreference;
    }

    private static void OnUserPreference(object? sender, UserPreferenceChangedEventArgs e)
    {
        // 主题变化会以 General 类别广播；这里不细分，跟随档下重新读一次最稳
        if (Mode == Follow) Apply(Follow);
    }
}
