using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32;

namespace MdFileAssistant.Shell;

/// <summary>
/// 「让本程序出现在 .md 的打开方式里」——只写 HKCU，不需要管理员，且可一键撤销。
///
/// 必须说清楚一件事：Windows 10/11 的**默认应用**由 UserChoice 哈希保护，
/// 第三方程序不能合法地静默改写。所以这里能做的是：
///   ① 注册 ProgID，让本程序出现在右键「打开方式」列表里；
///   ② 把系统设置页打开，让用户亲手点「设为默认」。
/// 界面文案照这个说实话，不写成「已设为默认」。
///
/// v0.1.4：整体从「spawn reg.exe + 解析控制台输出」改为直接用 .NET Registry API——
/// 不再有子进程（顺带消除了 reg 输出管道的死锁模式与串行 reg.exe 冻结 UI 的问题）、
/// 不再依赖 reg.exe 的输出编码、读写的值都是强类型。撤销前的「确实指向本程序」
/// 检查改为直接比对注册表里的字符串值；打开系统设置页改走 Win32 ShellExecuteW
/// 且参数全为常量。
/// </summary>
public static class FileAssociation
{
    public const string ProgId = "MdFileAssistant.Markdown";
    private const string ClassesSubKey = @"Software\Classes";
    private const string ProgSubKey = ClassesSubKey + @"\" + ProgId;
    private const string OpenCmdSubKey = ProgSubKey + @"\shell\open\command";
    private const string OpenWithSubKey =
        @"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.md\OpenWithProgids";

    private static string ExePath =>
        Environment.ProcessPath ?? System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName ?? "";

    private static string BackupDir =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MdFileAssistant");

    /// <summary>旧值快照（撤销不了时人可以照它手工恢复）。只记录，不自动回写。</summary>
    private sealed record Snapshot(bool Existed, string? OpenCommand);

    private static Snapshot ReadOpenCommand()
    {
        using var k = Registry.CurrentUser.OpenSubKey(OpenCmdSubKey);
        if (k == null) return new Snapshot(false, null);
        return new Snapshot(true, k.GetValue("") as string);
    }

    private static int CountOpenWith()
    {
        using var k = Registry.CurrentUser.OpenSubKey(OpenWithSubKey);
        return k?.ValueCount ?? 0;
    }

    public static bool IsRegistered()
    {
        using var k = Registry.CurrentUser.OpenSubKey(OpenCmdSubKey);
        return k?.GetValue("") is string s && s.Contains(ExePath, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>注册。返回一份人读的「写了什么 / 备份在哪 / 怎么撤销」报告。</summary>
    public static string Register()
    {
        var log = new StringBuilder();
        var exe = ExePath;
        if (string.IsNullOrWhiteSpace(exe) || !File.Exists(exe))
            return "找不到本程序的可执行文件路径，未做任何注册。";

        Directory.CreateDirectory(BackupDir);
        var stamp = DateTime.Now.ToString("yyyyMMdd-Hhmmsstt");

        // 备份旧值（若存在）：文本快照，供人手工恢复；不承诺自动回写
        var before = ReadOpenCommand();
        if (before.Existed && !string.IsNullOrEmpty(before.OpenCommand))
        {
            var bak = Path.Combine(BackupDir, $"assoc-{stamp}-OpenCommand.txt");
            try { File.WriteAllText(bak, before.OpenCommand, new UTF8Encoding(false)); log.AppendLine($"已备份原打开命令 -> {bak}"); }
            catch (Exception e) { log.AppendLine($"备份原打开命令失败（{e.Message}），继续注册。"); }
        }
        else
        {
            log.AppendLine("此前没有同名 ProgID 打开命令（无需备份）");
        }

        var beforeCount = CountOpenWith();
        var steps = new (string Name, Action Write)[]
        {
            ("ProgID 名称", () => Registry.CurrentUser.CreateSubKey(ProgSubKey).SetValue("", "MD文档阅读助手")),
            ("图标", () => Registry.CurrentUser.CreateSubKey(ProgSubKey + @"\DefaultIcon").SetValue("", $"\"{exe}\",0")),
            ("打开命令", () => Registry.CurrentUser.CreateSubKey(OpenCmdSubKey).SetValue("", $"\"{exe}\" \"%1\"")),
            (".md 的打开方式列表", () =>
            {
                using var k = Registry.CurrentUser.CreateSubKey(OpenWithSubKey);
                k.SetValue(ProgId, new byte[0], RegistryValueKind.None);
            }),
        };
        foreach (var (name, write) in steps)
        {
            try { write(); log.AppendLine($"✓ {name}"); }
            catch (Exception e) { log.AppendLine($"✗ {name}（失败：{e.Message}）"); }
        }
        log.AppendLine($"打开方式条目数：{beforeCount} -> {CountOpenWith()}");
        log.AppendLine("下一步：在弹出的系统设置里，把「MD文档阅读助手」设为 .md 的默认应用（这一步只能你亲手点）。");
        log.AppendLine("撤销：在本程序里点同一个按钮选「撤销」。");
        return log.ToString();
    }

    /// <summary>撤销：只删我们自己写的那两处，且必须先确认打开命令确实指向本程序。</summary>
    public static string Unregister()
    {
        var log = new StringBuilder();
        var cur = ReadOpenCommand();
        if (!cur.Existed || string.IsNullOrEmpty(cur.OpenCommand))
            return "没有找到本程序写入的注册项，无需撤销。";
        if (!cur.OpenCommand.Contains(ExePath, StringComparison.OrdinalIgnoreCase))
        {
            log.AppendLine("✗ 该 ProgID 的打开命令不指向本程序，为安全起见没有删除。");
            log.AppendLine("  现值：" + cur.OpenCommand);
            return log.ToString();
        }
        var beforeCount = CountOpenWith();
        try { Registry.CurrentUser.DeleteSubKeyTree(ProgSubKey); log.AppendLine("✓ 删除 ProgID 键"); }
        catch (Exception e) { log.AppendLine($"✗ 删除 ProgID 键（失败：{e.Message}）"); }
        try
        {
            using var k = Registry.CurrentUser.OpenSubKey(OpenWithSubKey, writable: true);
            k?.DeleteValue(ProgId);
            log.AppendLine("✓ 从 .md 打开方式列表移除");
        }
        catch (Exception e) { log.AppendLine($"✗ 从 .md 打开方式列表移除（失败：{e.Message}）"); }
        log.AppendLine($"打开方式条目数：{beforeCount} -> {CountOpenWith()}");
        return log.ToString();
    }

    /// <summary>打开系统「默认应用」设置页——这一步程序不能代劳。参数全为常量。</summary>
    public static void OpenSystemDefaults() =>
        NativeExec.ShellOpen("ms-settings:defaultapps");
}

/// <summary>Win32 ShellExecuteW 的最小封装：只用于「按关联打开」常量目标与零参数启动脚本。</summary>
internal static class NativeExec
{
    private const int SW_SHOWNORMAL = 1;
    private const int SW_HIDE = 0;

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr ShellExecuteW(IntPtr hwnd, string verb, string file, string? parameters,
        string? directory, int showCmd);

    public static void ShellOpen(string target) =>
        ShellExecuteW(IntPtr.Zero, "open", target, null, null, SW_SHOWNORMAL);

    public static void RunHidden(string scriptPath) =>
        ShellExecuteW(IntPtr.Zero, "open", scriptPath, null, null, SW_HIDE);
}
