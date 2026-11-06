using System.Text;
using MdFileAssistant.Reading;

// 被测对象是 Reading/ReadingRenderer.cs 正本里的 Join（csproj 里 Compile Include 过来的）。
int pass = 0, fail = 0;

void Check(string name, string actual, string expected)
{
    bool ok = actual == expected;
    if (ok) { pass++; Console.WriteLine($"PASS  {name}"); }
    else { fail++; Console.WriteLine($"FAIL  {name}\n      期望 {Show(expected)}\n      实际 {Show(actual)}"); }
}
static string Show(string s) => s.Replace("\n", "\\n");

// ① 截图里那份文档：六个「短标签：值」的字段行，不该被接成一段喘不上气的话
var fields = new List<string>
{
    "标题：他低头靠在护栏上，路边有人在喊他",
    "封面小字：那天好多人举着手机",
    "封面大字：喊他一声",
    "话题标签：#殡葬 #摆渡人辉哥 #人间百态 #长沙 #骑手 #敬畏生命",
    "置顶提问：你在路上有没有为一个人停过车？停下的那一次，你还记得什么",
    "发布窗口：19:30-20:00",
};
Check("字段行成对出现 · 每行各占一行",
      ReadingRenderer.Join(fields), string.Join("\n", fields));

// ② 核心 R11「并列项分行」插的换行：上一行以顿号/分号收尾，显示层不能再把它接回去
Check("顿号收尾 · 保留换行",
      ReadingRenderer.Join(new List<string> { "入选标准：强自洽、", "hook 信息密度高、", "能引发转发或评论争议" }),
      "入选标准：强自洽、\nhook 信息密度高、\n能引发转发或评论争议");
Check("分号收尾 · 保留换行",
      ReadingRenderer.Join(new List<string> { "第一条；", "第二条" }),
      "第一条；\n第二条");

// ③ 一句话被硬折开的，照旧接回去（这才是这个功能存在的理由，别改坏）
Check("中文句中被折开 · 接成一行且不补空格",
      ReadingRenderer.Join(new List<string> { "我是辉哥，干殡葬三十多年", "了。前几天看到一条视频。" }),
      "我是辉哥，干殡葬三十多年了。前几天看到一条视频。");
Check("中英交界处被折开 · 接回去补一个空格",
      ReadingRenderer.Join(new List<string> { "这条视频的 hook", "信息密度很高" }),
      "这条视频的 hook 信息密度很高");

// ④ 只有"两边都像字段行"才分行；一边像、一边不像就照旧接（中文之间不补空格是既有规矩）
Check("上一行不是字段行 · 接回去",
      ReadingRenderer.Join(new List<string> { "以下每条含发布文案和定稿口播", "改动说明附在每条末尾。" }),
      "以下每条含发布文案和定稿口播改动说明附在每条末尾。");

// ⑤ 时间戳与协议里的冒号不能被当成"字段行"（v0.1.4 在核心侧踩过同形坑，这里同理）
Check("数字开头的冒号不算字段行",
      ReadingRenderer.Join(new List<string> { "10:30 集合", "然后开始拍摄" }),
      "10:30 集合然后开始拍摄");
Check("URL 里的冒号不算字段行",
      ReadingRenderer.Join(new List<string> { "http://a.com/x：y", "参考这个链接" }),
      "http://a.com/x：y 参考这个链接");
// 「发布窗口：19:30-20:00」自己仍是字段行：冒号前是中文标签，不是数字
Check("标签里有时间戳 · 该行仍算字段行",
      ReadingRenderer.Join(new List<string> { "标题：靠护栏的骑手", "发布窗口：19:30-20:00" }),
      "标题：靠护栏的骑手\n发布窗口：19:30-20:00");

// ⑥ 原有的硬换行写法一条都不能退化；单个反斜杠是记号，兑现成换行后自己不该留在句末
Check("行尾两空格 · 硬换行照旧", ReadingRenderer.Join(new List<string> { "第一行  ", "第二行" }), "第一行\n第二行");
Check("行尾单反斜杠 · 换行且记号不上屏",
      ReadingRenderer.Join(new List<string> { "第一行\\", "第二行" }), "第一行\n第二行");
Check("行尾双反斜杠 · 是字面反斜杠，不分行",
      ReadingRenderer.Join(new List<string> { "末段\\\\", "第二行" }), "末段\\\\ 第二行");
Check("单行 · 原样返回", ReadingRenderer.Join(new List<string> { "只有一行" }), "只有一行");
Check("空列表 · 返回空串", ReadingRenderer.Join(new List<string>()), "");

Console.WriteLine($"\n==== {pass} passed, {fail} failed ====");
return fail;
