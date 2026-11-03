using System.Text;
using MdFileAssistant.Shell;

// 被测对象是 Shell/TextDecode.cs 正本（见 csproj 里的 Compile Include）。
// fixtures/ 下四份文件是刻意造的字节流样本：编码 × 换行风格 × BOM 各有分工。
var dir = Path.Combine(AppContext.BaseDirectory, "fixtures");
int pass = 0, fail = 0;

void Check(string name, bool ok, string detail = "")
{
    if (ok) { pass++; Console.WriteLine($"PASS  {name}" + (detail.Length > 0 ? "   " + detail : "")); }
    else { fail++; Console.WriteLine($"FAIL  {name}   {detail}"); }
}
static string Hex(byte[] b, int n) => Convert.ToHexString(b[..Math.Min(n, b.Length)]);

// 每一份都走「读进来 → 一个字不改 → 编回去」，要求逐字节还原。
// 标签写死在这里：识别结果一旦漂移，这条就会红，而不是悄悄换个标签继续自洽。
var cases = new (string File, string Label)[]
{
    ("测试-utf8-crlf.md", "UTF-8"),                 // 不转码：原始字节直接交给核心
    ("测试-utf8bom-crlf.md", "UTF-8"),              // 带 BOM：界面文本不许留 U+FEFF
    ("测试-utf16le.md", "UTF-16 LE（带 BOM）"),      // 转码：核心只吃 UTF-8
    ("测试-gbk.md", "GB18030（含 GBK）"),            // 转码：中文旧编码
};

foreach (var (file, wantLabel) in cases)
{
    var path = Path.Combine(dir, file);
    var raw = File.ReadAllBytes(path);
    var d = TextDecode.Decode(raw);
    if (d == null) { Check($"{file} · 能读", false, "Decode 返回 null"); continue; }
    Check($"{file} · 识别为 {wantLabel}", d.Label == wantLabel, $"实际 {d.Label} 首字符 U+{(int)d.Text[0]:X4}");

    var back = TextDecode.Encode(d.Text, d.Label, raw, out var err);
    Check($"{file} · 零改动写回逐字节还原", back != null && back.AsSpan().SequenceEqual(raw),
          err ?? $"长度 {back?.Length ?? -1} vs 原 {raw.Length}");

    // 改一行再写回：编码特征（BOM / UTF-16 前导）必须原样保留，且不能凭空多一份
    var edited = d.Text + "\r\n新增的一行。\r\n";
    var withChange = TextDecode.Encode(edited, d.Label, raw, out var err2);
    if (withChange == null) { Check($"{file} · 改动后仍可写回", false, err2 ?? ""); continue; }
    bool sameSignature = raw.Length >= 2
        ? withChange[..Math.Min(raw.Length, 2)].SequenceEqual(raw[..Math.Min(raw.Length, 2)])
        : true;
    Check($"{file} · 改动后保留原编码特征（前两字节一致）", sameSignature,
          $"新 {Hex(withChange, 6)} / 原 {Hex(raw, 6)}");
    Check($"{file} · 改动后变长且不含替换字符", withChange.Length > raw.Length && !edited.Contains('\uFFFD'));
}

// BOM 那一档单独盯死：0.2.0 的真 bug 就是存回去 BOM 没了（既不双份，也不整个丢掉）
var bomRaw = File.ReadAllBytes(Path.Combine(dir, "测试-utf8bom-crlf.md"));
var bomD = TextDecode.Decode(bomRaw)!;
Check("带 BOM 的 UTF-8 · 界面文本开头不留 U+FEFF", bomD.Text.Length > 0 && bomD.Text[0] != '\uFEFF',
      $"首字符 U+{(int)bomD.Text[0]:X4}");
var bomOut = TextDecode.Encode(bomD.Text + "x", "UTF-8", bomRaw, out _)!;
Check("带 BOM 的 UTF-8 · 写回恰好一个 BOM（0.2.0 会整个丢掉）",
      bomOut[..3].SequenceEqual(new byte[] { 0xEF, 0xBB, 0xBF }) && !bomOut[3..6].SequenceEqual(new byte[] { 0xEF, 0xBB, 0xBF }),
      "前 9 字节 " + Hex(bomOut, 9));

// 无 BOM 的 UTF-8 不许被补一个 BOM 出来
var plainRaw = File.ReadAllBytes(Path.Combine(dir, "测试-utf8-crlf.md"));
var plainOut = TextDecode.Encode(TextDecode.Decode(plainRaw)!.Text + "x", "UTF-8", plainRaw, out _)!;
Check("不带 BOM 的 UTF-8 · 写回也不许多出 BOM", !plainOut[..3].SequenceEqual(new byte[] { 0xEF, 0xBB, 0xBF }),
      "前 3 字节 " + Hex(plainOut, 3));

// 装不下的字：宁可拒写，不许换成别的字。
// 注意 GB18030 是全 Unicode 超集，emoji 它本来就能表示，所以拒写只会发生在纯 GBK(cp936) 上。
var gbkRaw = File.ReadAllBytes(Path.Combine(dir, "测试-gbk.md"));
var gbkD = TextDecode.Decode(gbkRaw)!;
var rejected = TextDecode.Encode(gbkD.Text + " 😀", "GBK", gbkRaw, out var gbkErr);
Check("GBK(cp936) · 写不出的字拒写并报出字符", rejected == null && gbkErr != null && gbkErr.Contains("😀"),
      (gbkErr ?? "<null>").Replace("\n", " "));
var kept = TextDecode.Encode(gbkD.Text, "GBK", gbkRaw, out var gbkErr2);
Check("GBK(cp936) · 原文一字不改仍可写回", kept != null && kept.AsSpan().SequenceEqual(gbkRaw),
      (gbkErr2 ?? $"长度 {kept?.Length ?? -1} vs 原 {gbkRaw.Length}"));
var as18030 = TextDecode.Encode(gbkD.Text + " 😀", "GB18030（含 GBK）", gbkRaw, out var err18030);
Check("GB18030 · emoji 可表示（超集，不该误拒）", as18030 != null, err18030 ?? "");

// 认不出的编码不许猜，也不许写
Check("含 NUL 的伪 PNG 拒读", TextDecode.Decode(new byte[] { 0x89, 0x50, 0x4E, 0x47, 0x00, 0x01 }) == null);
Check("标签不认识时拒写", TextDecode.Encode("x", "认不出", gbkRaw, out var e6) == null && e6 != null,
      (e6 ?? "").Replace("\n", " "));

Console.WriteLine($"\n==== {pass} passed, {fail} failed ====");
return fail;
