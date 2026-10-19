using System.Windows;

namespace MdFileAssistant;

public partial class App : Application
{
    /// <summary>双击 .md 或「打开方式」进来时，文件路径在命令行第 1 个参数里。</summary>
    public static string? StartupFile { get; private set; }

    protected override void OnStartup(StartupEventArgs e)
    {
        if (e.Args.Length > 0 && !string.IsNullOrWhiteSpace(e.Args[0]))
            StartupFile = e.Args[0];
        base.OnStartup(e);
    }
}
