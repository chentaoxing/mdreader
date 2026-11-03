using System.Windows;

namespace MdFileAssistant.Shell;

/// <summary>
/// 开启编辑前的确认窗。刻意做大、做慢：它要挡住的不是误点，是"我以为这个软件不会动我的文件"。
/// 默认按钮是取消，回车=取消；只有点最后那颗按钮才真的开编辑。
/// </summary>
public partial class EditWarningWindow : Window
{
    public bool Confirmed { get; private set; }

    public EditWarningWindow() => InitializeComponent();

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
