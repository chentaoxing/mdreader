using System.Windows;

namespace MdFileAssistant.Shell;

/// <summary>
/// 开启编辑前的确认窗。刻意做大、做慢：它要挡住的不是误点，是"我以为这个软件不会动我的文件"。
/// 默认按钮是取消，回车=取消；只有点最后那颗按钮才真的开编辑。
/// </summary>
public partial class EditWarningWindow : Window
{
    public bool Confirmed { get; private set; }

    public EditWarningWindow()
    {
        InitializeComponent();
        // 警告里那句"多久存一次"必须说的是真的，所以按当前设置现拼，不写死
        var s = Settings.Load();
        LblArc.Text = s.AutoArchive
            ? $"· 开着编辑时每 {s.AutoArchiveMinutes} 分钟会把当前内容写一份到缓存目录" +
              "（本地应用数据下的 MdFileAssistant\\autosave），关软件、崩溃都找得回来。间隔、目录、留几份都在「设置」里。"
            : "· 自动存档现在是关的（设置里可以打开）。开了之后，没保存的改动会定时写一份到缓存目录，关软件、崩溃都找得回来。";
    }

    private void OnCancel(object s, RoutedEventArgs e)
    {
        Confirmed = false;
        Close();
    }

    private void OnGo(object s, RoutedEventArgs e)
    {
        Confirmed = true;
        Close();
    }
}
