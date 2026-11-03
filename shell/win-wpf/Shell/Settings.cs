using System.IO;
using System.Text.Json;

namespace MdFileAssistant.Shell;

/// <summary>
/// 上次的阅读偏好 + 编辑与保存的规矩。存 %APPDATA%\MdFileAssistant\settings.json，
/// 纯文本、可直接删，不带任何注册表或后台服务。
///
/// 保存这一块的默认立场是「原件永远不被软件改写」：
/// 编辑后的保存默认落在源文件同目录的一个新文件里，定时存档落在缓存目录里的新文件里，
/// 只有你自己在设置里换成「覆盖源文件」才会动原件。
/// </summary>
public sealed class Settings
{
    public string Preset { get; set; } = "通用文章";
    public string Mode { get; set; } = "Hidden";
    public string Type { get; set; } = "标准";
    public string View { get; set; } = "对比源文件";
    public string Theme { get; set; } = "跟随系统";

    /// <summary>
    /// 启动 3 秒后静默查一次更新（照 ChinaSeal 的做法，默认开）。
    /// 这是本程序唯一的出网行为，取消勾选「启动时检查」后就只手动查。
    /// </summary>
    public bool AutoCheck { get; set; } = true;

    // ---- 编辑与保存 ------------------------------------------------------

    /// <summary>编辑后点「保存」去哪：同目录新文件（默认）/ 覆盖源文件。</summary>
    public string EditSaveTarget { get; set; } = SaveNewToSameDir;
    public const string SaveNewToSameDir = "同目录新文件";
    public const string SaveOverwriteSource = "覆盖源文件";

    /// <summary>「另存为」对话框默认打开哪个目录；空 = 源文件所在目录。</summary>
    public string SaveAsDir { get; set; } = "";

    /// <summary>定时把还没保存的内容写一份到缓存目录。默认开。</summary>
    public bool AutoArchive { get; set; } = true;
    public int AutoArchiveMinutes { get; set; } = 2;

    /// <summary>缓存目录；空 = %LOCALAPPDATA%\MdFileAssistant\autosave。</summary>
    public string AutoArchiveDir { get; set; } = "";

    /// <summary>同一个源文件在缓存目录里最多留几份，超了删最旧的。0 = 不清理。</summary>
    public int AutoArchiveKeep { get; set; } = 40;

    // ---- 弹窗 -----------------------------------------------------------

    /// <summary>点「编辑」时先弹那屏大字警告。关掉就直接进编辑。</summary>
    public bool WarnBeforeEdit { get; set; } = true;

    /// <summary>关软件时有未保存改动先问一句。无论选哪个，缓存目录那份都留着。</summary>
    public bool AskOnClose { get; set; } = true;

    // ---- 路径解析 -------------------------------------------------------

    public static string DefaultCacheDir => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "MdFileAssistant", "autosave");

    /// <summary>缓存目录：设置里给了就用设置的，没给用默认。只算路径，不建目录。</summary>
    public string CacheDirOrDefault =>
        string.IsNullOrWhiteSpace(AutoArchiveDir) ? DefaultCacheDir : AutoArchiveDir;

    /// <summary>「另存为」的起始目录：设置里给了就用设置的，没给或不存在就退回源文件所在目录。</summary>
    public string SaveAsDirOr(string sourcePath)
    {
        if (!string.IsNullOrWhiteSpace(SaveAsDir) && Directory.Exists(SaveAsDir)) return SaveAsDir;
        var near = Path.GetDirectoryName(sourcePath);
        return string.IsNullOrWhiteSpace(near) ? "" : near;
    }

    public bool OverwritesSource => EditSaveTarget == SaveOverwriteSource;

    private static string Path2 => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MdFileAssistant", "settings.json");

    public static Settings Load()
    {
        try
        {
            if (!File.Exists(Path2)) return new Settings();
            var s = JsonSerializer.Deserialize<Settings>(File.ReadAllText(Path2));
            if (s == null) return new Settings();
            // 设置文件被人手改坏时，宁可退回默认，也别让软件带着一个不认识的保存目标写盘
            if (s.EditSaveTarget != SaveNewToSameDir && s.EditSaveTarget != SaveOverwriteSource)
                s.EditSaveTarget = SaveNewToSameDir;
            if (s.AutoArchiveMinutes is < 1 or > 240) s.AutoArchiveMinutes = 2;
            if (s.AutoArchiveKeep is < 0 or > 5000) s.AutoArchiveKeep = 40;
            return s;
        }
        catch (Exception) { return new Settings(); }   // 设置坏了就用默认，不打扰用户
    }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path2)!);
            File.WriteAllText(Path2, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch (Exception) { /* 存不下不是错误，下次再说 */ }
    }
}
