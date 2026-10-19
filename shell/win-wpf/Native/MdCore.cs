using System.Buffers;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace MdFileAssistant.Native;

public sealed record AppliedRule(string RuleId, string RuleName, string Scope, int Edits, string Reason);
public sealed record RejectedRule(string RuleId, string RuleName, string Reason);
public sealed record Score(int Overall, int Heading, int Paragraph, int List, int Whitespace, int Density);
public sealed record RuleInfo(string Id, string Name, string Description, int Priority, string Safety, string Scope, bool AvailableInV1);
public sealed record Finding(int Line, string Dimension, string Detail);

/// <summary>一次 optimize / rebuild 的结果。Status 语义见 mdfile.h 的状态码表。</summary>
public sealed class OptimizeResult
{
    public int Status;
    public string? Error;
    public string Body = "";
    public byte[] Rendered = Array.Empty<byte>();
    public bool HadBom;
    public string NewlineStyle = "preserve";
    public int ContentChanges;
    public int StructuralChanges;
    public int Passes;
    public string ChecksumOriginal = "";
    public string ChecksumOptimized = "";
    public List<AppliedRule> Applied = new();
    public List<RejectedRule> Rejected = new();
    public List<(string Label, int Count)> Categories = new();
    public Score Before = new(0, 0, 0, 0, 0, 0);
    public Score After = new(0, 0, 0, 0, 0, 0);
    public string OutputName = "";
    public bool Ok => Status == 0;
    /// <summary>正文未变的机器凭据：比字符串，不比 JSON 里的数字（u64 超过 2^53 会丢末位）。</summary>
    public bool ContentUntouched => ContentChanges == 0 && ChecksumOriginal.Length > 0 && ChecksumOriginal == ChecksumOptimized;
}

public sealed class AnalyzeResult
{
    public int Status;
    public string? Error;
    public string Summary = "";
    public Score Score = new(0, 0, 0, 0, 0, 0);
    public int TotalLines, Headings, Paragraphs, ListItems, CodeLines, ContentTokens;
    public List<Finding> Findings = new();
    public bool Ok => Status == 0;
}

/// <summary>
/// crates/mdcore-ffi/include/mdfile.h 的 C# 绑定。这里没有任何 Markdown 语义判断，
/// 只做「字节进、JSON 出、内存按约定释放」。
/// </summary>
public static class MdCore
{
    private const string Dll = "mdcore_ffi.dll";

    public const int Ok = 0;
    public const int ErrUtf8 = 1;
    public const int ErrTooLarge = 2;
    public const int ErrGuard = 3;
    public const int ErrUnknown = 4;
    public const int ErrPanic = 5;
    public const int ErrArgs = -1;

    public static string StatusText(int code) => code switch
    {
        Ok => "OK",
        ErrUtf8 => "输入不是合法 UTF-8（核心绝不猜编码）",
        ErrTooLarge => "输入超过 64 MiB 上限",
        ErrGuard => "Content Guard 拒绝：正文发生了变化",
        ErrUnknown => "未知预设 / 未知规则 ID",
        ErrPanic => "核心内部异常（已捕获，未返回半成品）",
        ErrArgs => "非法参数",
        _ => $"未知状态码 {code}",
    };

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)] private static extern IntPtr mdfile_engine_version();
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)] private static extern IntPtr mdfile_last_error();
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)] private static extern void mdfile_free_string(IntPtr ptr);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int mdfile_optimize(byte[] input, UIntPtr inputLen, byte[]? preset, byte[]? newline, out IntPtr outJson, out UIntPtr outLen);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int mdfile_analyze(byte[] input, UIntPtr inputLen, byte[]? preset, out IntPtr outJson, out UIntPtr outLen);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int mdfile_preview(byte[] input, UIntPtr inputLen, byte[]? preset, byte[]? ruleId, out IntPtr outJson, out UIntPtr outLen);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int mdfile_rebuild(byte[] original, UIntPtr originalLen, byte[]? preset, byte[]? decisionsJson, out IntPtr outJson, out UIntPtr outLen);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int mdfile_rules_json(out IntPtr outJson, out UIntPtr outLen);

    public static bool Available { get; } = Probe();

    private static bool Probe()
    {
        try { return !string.IsNullOrEmpty(EngineVersion); }
        catch (Exception) { return false; }
    }

    public static string EngineVersion
    {
        get
        {
            // 静态串，不释放
            var p = mdfile_engine_version();
            return p == IntPtr.Zero ? "" : (Marshal.PtrToStringUTF8(p) ?? "");
        }
    }

    /// <summary>取最近一次调用的错误消息。返回值总是堆上的，取完必须释放。</summary>
    public static string LastError()
    {
        var p = mdfile_last_error();
        if (p == IntPtr.Zero) return "";
        var s = Marshal.PtrToStringUTF8(p) ?? "";
        mdfile_free_string(p);   // 无错误时也是堆内存，可以安全释放（旧实现返回静态指针，照文档释放会崩）
        return s;
    }

    private static byte[]? Utf8Z(string? s) => s is null ? null : Encoding.UTF8.GetBytes(s + "\0");

    private static string? TakeJson(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero) return null;
        var s = Marshal.PtrToStringUTF8(ptr);
        mdfile_free_string(ptr);
        return s;
    }

    public static OptimizeResult Optimize(byte[] input, string preset, string newline)
    {
        var code = mdfile_optimize(input, (UIntPtr)(ulong)input.LongLength, Utf8Z(preset), Utf8Z(newline), out var json, out _);
        return ParseOptimize(code, TakeJson(json));
    }

    /// <summary>按「规则 → 接受/拒绝」的决策重建正文。decisions 为空表示全部接受。</summary>
    public static OptimizeResult Rebuild(byte[] input, string preset, IEnumerable<(string RuleId, bool Accept)> decisions)
    {
        var list = decisions.Select(d => new { decision = d.Accept ? "accept" : "reject", rule_id = d.RuleId }).ToList();
        var dj = JsonSerializer.Serialize(list);
        var code = mdfile_rebuild(input, (UIntPtr)(ulong)input.LongLength, Utf8Z(preset), Utf8Z(dj), out var json, out _);
        return ParseRebuild(code, TakeJson(json));
    }

    public static AnalyzeResult Analyze(byte[] input, string preset)
    {
        var code = mdfile_analyze(input, (UIntPtr)(ulong)input.LongLength, Utf8Z(preset), out var raw, out _);
        var json = TakeJson(raw);   // 返回的是指针，必须先取字符串再释放
        var r = new AnalyzeResult { Status = code };
        if (code != Ok) { r.Error = LastError(); return r; }
        using var doc = JsonDocument.Parse(json ?? "{}");
        var root = doc.RootElement;
        r.Summary = root.TryGetProp("summary", out var sm) ? sm.GetString() ?? "" : "";
        r.Score = ReadScore(root, "score");
        r.TotalLines = root.TryGetProp("total_lines", out var tl) ? tl.GetInt32() : 0;
        r.Headings = root.TryGetProp("headings", out var hd) ? hd.GetInt32() : 0;
        r.Paragraphs = root.TryGetProp("paragraphs", out var pg) ? pg.GetInt32() : 0;
        r.ListItems = root.TryGetProp("list_items", out var li) ? li.GetInt32() : 0;
        r.CodeLines = root.TryGetProp("code_lines", out var cl) ? cl.GetInt32() : 0;
        r.ContentTokens = root.TryGetProp("content_tokens", out var ct) ? ct.GetInt32() : 0;
        if (root.TryGetProperty("findings", out var fs) && fs.ValueKind == JsonValueKind.Array)
            foreach (var f in fs.EnumerateArray())
                r.Findings.Add(new Finding(
                    f.TryGetProp("line", out var ln) ? ln.GetInt32() : 0,
                    f.TryGetProp("dimension", out var dm) ? dm.GetString() ?? "" : "",
                    f.TryGetProp("detail", out var dt) ? dt.GetString() ?? "" : ""));
        return r;
    }

    public static List<RuleInfo> Rules(out List<string> presets)
    {
        presets = new();
        var code = mdfile_rules_json(out var raw, out _);
        var json = TakeJson(raw);
        var list = new List<RuleInfo>();
        if (code != Ok) { LastError(); return list; }
        using var doc = JsonDocument.Parse(json ?? "{}");
        var root = doc.RootElement;
        if (root.TryGetProperty("presets", out var ps) && ps.ValueKind == JsonValueKind.Array)
            foreach (var p in ps.EnumerateArray())
                if (p.ValueKind == JsonValueKind.Array && p.GetArrayLength() > 0)
                    presets.Add(p[0].GetString() ?? "");
        if (root.TryGetProperty("rules", out var rs) && rs.ValueKind == JsonValueKind.Array)
            foreach (var r in rs.EnumerateArray())
                list.Add(new RuleInfo(
                    r.TryGetProp("id", out var id) ? id.GetString() ?? "" : "",
                    r.TryGetProp("name", out var nm) ? nm.GetString() ?? "" : "",
                    r.TryGetProp("description", out var de) ? de.GetString() ?? "" : "",
                    r.TryGetProp("priority", out var pr) ? pr.GetInt32() : 0,
                    r.TryGetProp("safety", out var sf) ? sf.GetString() ?? "" : "",
                    r.TryGetProp("scope", out var sc) ? sc.GetString() ?? "" : "",
                    !r.TryGetProp("available_in_v1", out var av) || av.GetBoolean()));
        return list;
    }

    private static OptimizeResult ParseOptimize(int code, string? json) => Fill(new OptimizeResult { Status = code }, code, json, isRebuild: false);
    private static OptimizeResult ParseRebuild(int code, string? json) => Fill(new OptimizeResult { Status = code }, code, json, isRebuild: true);

    private static OptimizeResult Fill(OptimizeResult r, int code, string? json, bool isRebuild)
    {
        if (code != Ok || string.IsNullOrEmpty(json))
        {
            r.Error = string.IsNullOrWhiteSpace(LastError()) ? StatusText(code) : LastError();
            return r;
        }
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;
        r.Body = root.TryGetProp("body", out var b) ? b.GetString() ?? "" : "";
        r.HadBom = root.TryGetProp("had_bom", out var hb) && hb.GetBoolean();
        r.NewlineStyle = root.TryGetProp("newline_style", out var ns) ? ns.GetString() ?? "preserve" : "preserve";
        r.ContentChanges = root.TryGetProp("content_changes", out var cc) ? cc.GetInt32() : 0;
        r.StructuralChanges = root.TryGetProp("structural_changes", out var st) ? st.GetInt32() : r.ContentChanges;
        r.Passes = root.TryGetProp("passes", out var pa) ? pa.GetInt32() : 0;
        r.ChecksumOriginal = ReadChecksum(root, isRebuild ? null : "checksum_original", "checksum_original_str");
        // rebuild 只回一个 checksum_str（重建后正文的校验和），界面拿它去比 optimize 给出的原始校验和
            r.ChecksumOptimized = ReadChecksum(root, isRebuild ? null : "checksum_optimized",
                "checksum_optimized_str", "checksum_str");
        if (root.TryGetProp("rendered_latin1", out var rl) && rl.ValueKind == JsonValueKind.String)
            r.Rendered = Latin1ToBytes(rl.GetString() ?? "");
        if (!isRebuild)
        {
            r.Before = ReadScore(root, "score_before");
            r.After = ReadScore(root, "score_after");
            r.Applied = ReadApplied(root);
            r.Rejected = ReadRejected(root);
            r.Categories = ReadCategories(root);
            if (root.TryGetProp("manifest_json", out var mj) && mj.ValueKind == JsonValueKind.String)
            {
                try
                {
                    using var md = JsonDocument.Parse(mj.GetString() ?? "{}");
                    if (md.RootElement.TryGetProp("output_name", out var on)) r.OutputName = on.GetString() ?? "";
                }
                catch (JsonException) { /* 清单只是附带信息，坏了不影响主流程 */ }
            }
        }
        else
        {
            // rebuild 只回正文与校验和；改动数按「与原文相比」由界面自己判断
            r.Before = r.After = new Score(0, 0, 0, 0, 0, 0);
        }
        return r;
    }

    private static string ReadChecksum(JsonElement root, string? numericKey, params string[] strKeys)
    {
        foreach (var k in strKeys)
            if (k != null && root.TryGetProp(k, out var v) && v.ValueKind == JsonValueKind.String)
                return v.GetString() ?? "";
        if (numericKey != null && root.TryGetProp(numericKey, out var n) && n.ValueKind == JsonValueKind.Number)
            return n.ToString();
        return "";
    }

    private static Score ReadScore(JsonElement root, string key)
    {
        if (!root.TryGetProp(key, out var s) || s.ValueKind != JsonValueKind.Object) return new Score(0, 0, 0, 0, 0, 0);
        int Get(string k) => s.TryGetProp(k, out var v) && v.ValueKind == JsonValueKind.Number ? v.GetInt32() : 0;
        return new Score(Get("overall"), Get("heading"), Get("paragraph"), Get("list"), Get("whitespace"), Get("density"));
    }

    private static List<AppliedRule> ReadApplied(JsonElement root)
    {
        var list = new List<AppliedRule>();
        if (!root.TryGetProp("applied", out var a) || a.ValueKind != JsonValueKind.Array) return list;
        foreach (var x in a.EnumerateArray())
            list.Add(new AppliedRule(
                x.TryGetProp("rule_id", out var i) ? i.GetString() ?? "" : "",
                x.TryGetProp("rule_name", out var n) ? n.GetString() ?? "" : "",
                x.TryGetProp("scope", out var s) ? s.GetString() ?? "" : "",
                x.TryGetProp("edits", out var e) ? e.GetInt32() : 0,
                x.TryGetProp("reason", out var r) ? r.GetString() ?? "" : ""));
        return list;
    }

    private static List<RejectedRule> ReadRejected(JsonElement root)
    {
        var list = new List<RejectedRule>();
        if (!root.TryGetProp("rejected", out var a) || a.ValueKind != JsonValueKind.Array) return list;
        foreach (var x in a.EnumerateArray())
            list.Add(new RejectedRule(
                x.TryGetProp("rule_id", out var i) ? i.GetString() ?? "" : "",
                x.TryGetProp("rule_name", out var n) ? n.GetString() ?? "" : "",
                x.TryGetProp("reason", out var r) ? r.GetString() ?? "" : ""));
        return list;
    }

    private static List<(string, int)> ReadCategories(JsonElement root)
    {
        var list = new List<(string, int)>();
        if (!root.TryGetProp("categories", out var a) || a.ValueKind != JsonValueKind.Array) return list;
        foreach (var x in a.EnumerateArray())
            if (x.ValueKind == JsonValueKind.Array && x.GetArrayLength() == 2)
                list.Add((x[0].GetString() ?? "", x[1].GetInt32()));
        return list;
    }

    /// <summary>rendered_latin1 的第 n 个字符就是文件的第 n 个字节。</summary>
    private static byte[] Latin1ToBytes(string s)
    {
        var buf = ArrayPool<byte>.Shared.Rent(s.Length == 0 ? 1 : s.Length);
        try
        {
            for (var i = 0; i < s.Length; i++)
            {
                var c = s[i];
                if (c > 0xFF) throw new InvalidDataException($"latin1 通道出现 U+{((int)c):X4}");
                buf[i] = (byte)c;
            }
            return buf.AsSpan(0, s.Length).ToArray();
        }
        finally { ArrayPool<byte>.Shared.Return(buf); }
    }
}

internal static class JsonExt
{
    /// <summary>snake_case 字段直取；取不到时返回 false，调用方自己决定默认值。</summary>
    public static bool TryGetProp(this JsonElement e, string name, out JsonElement v)
    {
        v = default;
        return e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out v);
    }
}
