using System.Windows;
using MdFileAssistant.Shell;
using MdFileAssistant.Theme;

namespace MdFileAssistant;

public partial class App : Application
{
    /// <summary>双击 .md 或「打开方式」进来时，文件路径在命令行第 1 个参数里。</summary>
    public static string? StartupFile { get; private set; }

    protected override void OnStartup(StartupEventArgs e)
    {
        if (e.Args.Length > 0 && !string.IsNullOrWhiteSpace(e.Args[0]))
            StartupFile = e.Args[0];

        // 先把主题落到 Application.Resources，再让 StartupUri 建窗口：
        // 顺序反了会先闪一帧浅色再变深。
        ThemeManager.HookSystemChanges();
        ThemeManager.Apply(Settings.Load().Theme);
        base.OnStartup(e);
    }
}
