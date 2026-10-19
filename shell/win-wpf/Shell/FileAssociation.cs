using System.Diagnostics;
using System.IO;
using System.Text;

namespace MdFileAssistant.Shell;

/// <summary>
/// 「让本程序出现在 .md 的打开方式里」——只写 HKCU，不需要管理员，且可一键撤销。
///
/// 必须说清楚一件事：Windows 10/11 的**默认应用**由 UserChoice 哈希保护，
/// 第三方程序不能合法地静默改写。所以这里能做的是：
///   ① 注册 ProgID，让本程序出现在右键「打开方式」列表里；
///   ② 把系统设置页打开，让用户亲手点「设为默认」。
/// 界面文案照这个说实话，不写成「已设为默认」。
/// </summary>
public static class FileAssociation
{
    public const string ProgId = "MdFileAssistant.Markdown";
    private const string ClassesRoot = @"HKEY_CURRENT_USER\Software\Classes";
    private const string ProgKey = ClassesRoot + @"\" + ProgId;
    private const string OpenCmdKey = ProgKey + @"\shell\open\command";
    private const string OpenWithProgIds =
        @"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.md\OpenWithProgids";

    private static string ExePath =>
        Environment.ProcessPath ?? Process.GetCurrentProcess().MainModule?.FileName ?? "";

    private static string BackupDir =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MdFileAssistant");

    private static string RegExe =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "reg.exe");

    /// <summary>用 ArgumentList 传参：引号由运行时按标准规则转义，不手搓字符串拼接。</summary>
    private static (int Code, string Out) Run(params string[] args)
    {
        var psi = new ProcessStartInfo(RegExe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8,
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        using var p = Process.Start(psi) ?? throw new InvalidOperationException("无法启动 reg.exe");
        var o = p.StandardOutput.ReadToEnd() + p.StandardError.ReadToEnd();
        p.WaitForExit(8000);
        return (p.HasExited ? p.ExitCode : -1, o);
    }

    public static bool IsRegistered() => Run("query", OpenCmdKey, "/v", "").Code == 0;

    /// <summary>注册。返回一份人读的「写了什么 / 备份在哪 / 怎么撤销」报告。</summary>
    public static string Register()
    {
        var log = new StringBuilder();
        var exe = ExePath;
        if (string.IsNullOrWhiteSpace(exe) || !File.Exists(exe))
            return "找不到本程序的可执行文件路径，未做任何注册。";

        Directory.CreateDirectory(BackupDir);
        var stamp = DateTime.Now.ToString("yyyyMMdd-HHmmss");
        var bakProg = Path.Combine(BackupDir, $"assoc-{stamp}-ProgID.reg");
        var bakExt = Path.Combine(BackupDir, $"assoc-{stamp}-OpenWith.reg");

        var b1 = Run("export", ProgKey, bakProg, "/y");
        var b2 = Run("export", OpenWithProgIds, bakExt, "/y");
        log.AppendLine(b1.Code == 0 ? $"已备份同名 ProgID 键 -> {bakProg}" : "此前没有同名 ProgID 键（无需备份）");
        log.AppendLine(b2.Code == 0 ? $"已备份 .md 的 OpenWithProgids -> {bakExt}" : "此前没有 .md 的 OpenWithProgids 记录（无需备份）");

        var before = CountOpenWith();
        var steps = new (string Name, string[] Args)[]
        {
            ("ProgID 名称", new[] { "add", ProgKey, "/ve", "/d", "MD文档阅读助手", "/f" }),
            ("图标", new[] { "add", ProgKey + @"\DefaultIcon", "/ve", "/d", $"\"{exe}\",0", "/f" }),
            ("打开命令", new[] { "add", OpenCmdKey, "/ve", "/d", $"\"{exe}\" \"%1\"", "/f" }),
            (".md 的打开方式列表", new[] { "add", OpenWithProgIds, "/v", ProgId, "/t", "REG_NONE", "/f" }),
        };
        foreach (var (name, args) in steps)
        {
            var (code, _) = Run(args);
            log.AppendLine($"{(code == 0 ? "✓" : "✗")} {name}{(code == 0 ? "" : "（失败）")}");
        }
        log.AppendLine($"打开方式条目数：{before} -> {CountOpenWith()}");
        log.AppendLine("下一步：在弹出的系统设置里，把「MD文档阅读助手」设为 .md 的默认应用（这一步只能你亲手点）。");
        log.AppendLine($"撤销：在本程序里点同一个按钮选「撤销」，或运行 reg delete \"{ProgKey}\" /f");
        return log.ToString();
    }

    /// <summary>撤销：只删我们自己写的那两处，且必须先确认打开命令确实指向本程序。</summary>
    public static string Unregister()
    {
        var log = new StringBuilder();
        var (code, outp) = Run("query", OpenCmdKey, "/v", "");
        if (code != 0) return "没有找到本程序写入的注册项，无需撤销。";
        if (!outp.Contains(ExePath, StringComparison.OrdinalIgnoreCase))
        {
            log.AppendLine("✗ 该 ProgID 的打开命令不指向本程序，为安全起见没有删除。");
            log.AppendLine("  现值：" + outp.Split('\n').LastOrDefault(l => l.Contains("REG_SZ"))?.Trim());
            return log.ToString();
        }
        var before = CountOpenWith();
        var d1 = Run("delete", ProgKey, "/f");
        var d2 = Run("delete", OpenWithProgIds, "/v", ProgId, "/f");
        log.AppendLine($"{(d1.Code == 0 ? "✓" : "✗")} 删除 ProgID 键");
        log.AppendLine($"{(d2.Code == 0 ? "✓" : "✗")} 从 .md 打开方式列表移除");
        log.AppendLine($"打开方式条目数：{before} -> {CountOpenWith()}");
        return log.ToString();
    }

    /// <summary>打开系统「默认应用」设置页——这一步程序不能代劳。</summary>
    public static void OpenSystemDefaults()
    {
        try { Process.Start(new ProcessStartInfo("ms-settings:defaultapps") { UseShellExecute = true }); }
        catch (Exception e) { Debug.WriteLine("打不开系统设置：" + e.Message); }
    }

    private static int CountOpenWith()
    {
        var (code, outp) = Run("query", OpenWithProgIds);
        return code != 0 ? 0 : outp.Split('\n').Count(l => l.Contains("REG_"));
    }
}
