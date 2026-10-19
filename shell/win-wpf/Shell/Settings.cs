using System.IO;
using System.Text.Json;

namespace MdFileAssistant.Shell;

/// <summary>
/// 记住上次的预设与阅读偏好。存 %APPDATA%\MdFileAssistant\settings.json，
/// 纯文本、可直接删，不带任何注册表或后台服务。
/// 这里只存「怎么读」，不存任何跟文件内容有关的东西。
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

    private static string Path2 => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MdFileAssistant", "settings.json");

    public static Settings Load()
    {
        try
        {
            if (!File.Exists(Path2)) return new Settings();
            var s = JsonSerializer.Deserialize<Settings>(File.ReadAllText(Path2));
            return s ?? new Settings();
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
