using System.Runtime.InteropServices;
using System.Text;

namespace MdFileAssistant.Shell;

/// <summary>
/// 打开文件时的编码判定。核心只吃 UTF-8（这是五平台逐字节一致的前提，不能为了兼容去动它），
/// 所以这里只做一件安全的事：**在内存里**把 GBK / UTF-16 转成 UTF-8 副本交给核心，
/// 原文件的字节一个都不动，另存出去的也是一份 UTF-8 新文件，界面会把这件事写在状态条上。
///
/// 判定顺序是"能确定才动手"：BOM 明确 → 按 BOM；严格 UTF-8 能解 → 就是 UTF-8；
/// 都不行才试 GBK / GB18030；再不行返回 null，由界面按"读不了"处理，绝不猜一个编码糊上去。
/// </summary>
public static class TextDecode
{
    public sealed record Result(string Text, string Label, byte[] Utf8, bool Transcoded);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern int MultiByteToWideChar(uint codePage, uint flags, byte[] multi, int count, IntPtr wide, int wideCount);

    private const uint CpGbk = 936;
    private const uint CpGB18030 = 54936;

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
}
