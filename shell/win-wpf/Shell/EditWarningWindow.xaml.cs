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
        // 这屏警告里每一句涉及"保存去哪、多久存一次、会不会重读磁盘"的话都必须跟着当前设置走。
        // 0.2.0 就是写死了"默认写到旁边一个新文件"，用户已经在设置里选了覆盖源文件，
        // 弹窗还在承诺不会动原件——这是最要命的一句假话，偏偏长在最显眼的标题上。
        var s = Settings.Load();
        bool over = s.OverwritesSource;

        LblHead.Text = over
            ? "开编辑会动你的源文件——设置里选的是「覆盖源文件」。"
            : "开编辑不会动你的源文件——保存默认写到旁边一个新文件。";
        LblLead.Text = over
            ? "左栏从此可以打字。你改的内容，点「保存回源文件」就写进你打开的那个文件本身。" +
              "这是你在「设置」里选了「覆盖源文件」的结果；想回到不动原件的做法，去设置里改回「同目录新文件」。"
            : "左栏从此可以打字。你改的内容，按「另存为新文件」写到原件旁边，原件一个字节都不动。" +
              "想直接写回原件，得去「设置」里把保存目标改成「覆盖源文件」——那是另一个决定。";
        LblSaveBullet.Text = over
            ? "· 左栏变成可输入。改动要点「保存回源文件」才落盘，落进的就是你打开的那个文件；不点就什么都没发生。"
            : "· 左栏变成可输入。改动要点「另存为新文件」才落盘，落在原件旁边一个带时间的新文件；不点就什么都没发生。";
        LblOverwriteBullet.Text = over
            ? "· 现在选的是「覆盖源文件」：第一次写回之前会先复制一份 .bak（已存在就不覆盖），并且落盘前重读磁盘核对。"
            : "· 只有你在「设置」里把保存目标改成「覆盖源文件」，才会真的写回原件——那之前会先复制一份 .bak（已存在就不覆盖），并且落盘前重读磁盘核对。";
        LblConflictBullet.Text = over
            ? "· 落盘前会重读一次磁盘。文件在你打开之后被别的程序改过，就拒绝写，让你先决定。"
            : "· 写「新文件」不动原件，所以不存在覆盖别人改动的风险；改成「覆盖源文件」后才会做落盘前重读核对。";
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
