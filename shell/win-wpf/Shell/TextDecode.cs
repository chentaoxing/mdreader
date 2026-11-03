using System.Runtime.InteropServices;
using System.Text;

namespace MdFileAssistant.Shell;

/// <summary>
/// 文件的读与写回，成对放在这里，是为了让"按什么编码读进来，就得按什么编码写回去"这件事
/// 没法被分开改错。
///
/// 读：核心只吃 UTF-8（这是五平台逐字节一致的前提，不能为了兼容去动它），
/// 所以判定后**在内存里**把 GBK / UTF-16 转成 UTF-8 副本交给核心，原文件的字节一个都不动。
/// 判定顺序是"能确定才动手"：BOM 明确 → 按 BOM；严格 UTF-8 能解 → 就是 UTF-8；
/// 都不行才试 GBK / GB18030；再不行返回 null，由界面按"读不了"处理，绝不猜一个编码糊上去。
///
/// 写（编辑模式）：一律按读进来那个 Label 原样编回去，不悄悄转码。
/// 中文编码写不出的字直接拒写，不用 '?' 糊过去——那是把用户的字弄丢。
/// </summary>
public static class TextDecode
{
    public sealed record Result(string Text, string Label, byte[] Utf8, bool Transcoded);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern int MultiByteToWideChar(uint codePage, uint flags, byte[] multi, int count, IntPtr wide, int wideCount);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern int WideCharToMultiByte(uint codePage, uint flags,
        [MarshalAs(UnmanagedType.LPWStr)] string wide, int wideCount,
        byte[]? multi, int multiCount, IntPtr defaultChar,
        [MarshalAs(UnmanagedType.Bool)] out bool usedDefault);

    private const uint CpGbk = 936;
    private const uint CpGB18030 = 54936;
    private const uint WcNoBestFit = 0x400;   // 关掉近似映射：写不出就报，别偷偷换成别的字

    public static Result? Decode(byte[] raw)
    {
        if (raw.Length >= 2 && raw[0] == 0xFF && raw[1] == 0xFE)
            return FromManaged(Encoding.Unicode.GetString(raw, 2, raw.Length - 2), "UTF-16 LE（带 BOM）");
        if (raw.Length >= 2 && raw[0] == 0xFE && raw[1] == 0xFF)
            return FromManaged(Encoding.BigEndianUnicode.GetString(raw, 2, raw.Length - 2), "UTF-16 BE（带 BOM）");

        // 带 NUL 的字节流不是文本（UTF-16 已在上面按 BOM 处理掉了），不做任何猜测
        if (raw.Contains((byte)0)) return null;

        // UTF-8（含带 BOM 的）：字节原样交给核心，行为与之前完全一致，不转码
        try
        {
            new UTF8Encoding(false, throwOnInvalidBytes: true).GetString(raw);
            return new Result(Encoding.UTF8.GetString(raw), "UTF-8", raw, false);
        }
        catch (ArgumentException) { /* 不是合法 UTF-8，往下试中文编码 */ }

        // GB18030 是 GBK 的超集，且 2 字节段与 GBK 完全一致：先试超集，四字节生僻字才不会解坏
        foreach (var (cp, label) in new[] { (CpGB18030, "GB18030（含 GBK）"), (CpGbk, "GBK") })
        {
            var s = FromMultiByte(raw, cp);
            if (s != null) return FromManaged(s, label);
        }
        return null;   // 认不出来的编码不猜，让界面明说读不了
    }

    /// <summary>
    /// 用 kernel32 把中文编码解成 UTF-16。缓冲区自己分配、自己拷回来：
    /// 直接把 `char[]` 交给这个 API 时，实测拿回来的是整屏 NUL（长度对、内容全空），
    /// 界面就会显示成"文件是空的"——所以这里不走托管数组那条路。
    /// </summary>
    private static string? FromMultiByte(byte[] raw, uint cp)
    {
        if (raw.Length == 0) return "";
        int need = MultiByteToWideChar(cp, 0, raw, raw.Length, IntPtr.Zero, 0);
        if (need <= 0) return null;
        IntPtr buf = Marshal.AllocHGlobal(need * 2);
        try
        {
            int got = MultiByteToWideChar(cp, 0, raw, raw.Length, buf, need);
            if (got <= 0) return null;
            var chars = new char[got];
            Marshal.Copy(buf, chars, 0, got);
            var s = new string(chars);
            if (s.Contains('\0')) return null;        // 解出一堆 NUL 就是解错了，不交给核心
            int bad = s.Count(c => c == '\uFFFD');
            return bad > Math.Max(2, s.Length / 1000) ? null : s;   // 替换符太多=拿二进制当中文读，不认
        }
        finally { Marshal.FreeHGlobal(buf); }
    }

    private static Result FromManaged(string text, string label)
    {
        // 转出来的副本一律不带 BOM：核心自己会记录并保留换行风格
        var utf8 = new UTF8Encoding(false).GetBytes(text);
        return new Result(text, label, utf8, true);
    }

    /// <summary>
    /// 按打开时判出的那个编码编回去。<paramref name="original"/> 只用来抄 BOM 的有无。
    /// 返回 null 表示"这份内容写不回这个编码"，<paramref name="error"/> 是人话原因，界面直接显示。
    /// </summary>
    public static byte[]? Encode(string text, string label, byte[] original, out string? error)
    {
        error = null;
        switch (label)
        {
            case "UTF-8":
            {
                bool hasBom = original.Length >= 3 && original[0] == 0xEF && original[1] == 0xBB && original[2] == 0xBF;
                return new UTF8Encoding(hasBom).GetBytes(text);
            }
            case "UTF-16 LE（带 BOM）":
                return Concat(new byte[] { 0xFF, 0xFE }, Encoding.Unicode.GetBytes(text));
            case "UTF-16 BE（带 BOM）":
                return Concat(new byte[] { 0xFE, 0xFF }, Encoding.BigEndianUnicode.GetBytes(text));
            case "GB18030（含 GBK）":
                return ToCodePage(text, CpGB18030, label, out error);
            case "GBK":
                return ToCodePage(text, CpGbk, label, out error);
            default:
                error = $"这份文件当初是按「{label}」读的，说不清该按什么编码写回去，所以不写。";
                return null;
        }
    }

    private static byte[]? ToCodePage(string text, uint cp, string label, out string? error)
    {
        error = null;
        if (text.Length == 0) return Array.Empty<byte>();
        int need = WideCharToMultiByte(cp, WcNoBestFit, text, text.Length, null, 0, IntPtr.Zero, out _);
        if (need <= 0)
        {
            error = $"这个编码（{label}）连长度都算不出来，写不回去。";
            return null;
        }
        var buf = new byte[need];
        bool usedDefault;
        int got = WideCharToMultiByte(cp, WcNoBestFit, text, text.Length, buf, need, IntPtr.Zero, out usedDefault);
        if (got <= 0 || usedDefault)
        {
            var bad = FirstUnmappable(text, cp);
            error = $"这份文件是 {label}，你新打的字符 {bad} 在这个编码里根本没有对应的字节。" +
                    "要么别打这个字符，要么另存成 UTF-8 的新文件——我不会把它换成问号写进你的文件。";
            return null;
        }
        return buf;
    }

    /// <summary>找出第一个在该代码页里写不出的字符，给报错用；找不到就回一个笼统说法。</summary>
    private static string FirstUnmappable(string text, uint cp)
    {
        for (int i = 0; i < text.Length; i++)
        {
            var one = text[i].ToString();
            if (WideCharToMultiByte(cp, WcNoBestFit, one, 1, null, 0, IntPtr.Zero, out _) <= 0) continue;
            var probe = new byte[8];
            if (WideCharToMultiByte(cp, WcNoBestFit, one, 1, probe, probe.Length, IntPtr.Zero, out bool used) > 0 && !used)
                continue;
            return $"「{one}」(U+{(int)text[i]:X4})";
        }
        return "某个字符";
    }

    private static byte[] Concat(byte[] head, byte[] body)
    {
        var r = new byte[head.Length + body.Length];
        Buffer.BlockCopy(head, 0, r, 0, head.Length);
        Buffer.BlockCopy(body, 0, r, head.Length, body.Length);
        return r;
    }
}
