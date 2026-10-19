using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace MdFileAssistant.Shell;

/// <summary>
/// 自更新：检查 → 下载 → 交给 helper.bat 覆盖重启。口径照 ChinaSeal 的 updater.py 移植。
///
/// 三条不能省的规矩：
/// 1. **一切出网先过 <see cref="NetGuard"/>**：https、白名单主机、解析 IP 不得是内网/回环/保留段，
///    重定向逐跳再校验一遍。
/// 2. **换源 + 看门狗**：某个源 15 秒零进度（停滞）或不足 15MB（龟速）就切下一个候选源；
///    最后一个源容忍龟速磨完。中文网络下 GitHub 常是"能连上但磨"，这条决定能不能更新成功。
/// 3. **落盘只准进 %TEMP%\MDReader-Update**，分块写各自旁路文件、带轮次号，最后合并 + 校验尺寸。
///
/// 触发方式：界面上的「检查更新」按钮。启动时不联网（默认档见 <see cref="Settings.AutoCheck"/>）。
/// </summary>
public static class Updater
{
    /// <summary>发布仓库。换仓只改这一行。</summary>
    public const string Repo = "chentaoxing/mdreader";

    /// <summary>更新包里认这个文件名（和 ChinaSeal 的 portable.zip 同一套思路）。</summary>
    private const string AssetHint = "portable.zip";

    private static readonly TimeSpan CheckTimeout = TimeSpan.FromSeconds(25);
    private const int StallWindowSec = 15;
    private const long SlowBytes = 15L * 1024 * 1024;     // 15 秒窗口内不足 15MB 判龟速
    private const int ParallelChunks = 4;
    private const long MinPackageBytes = 512 * 1024;

    public sealed record CheckResult(bool Ok, string Current, string? Latest, bool Available,
                                     string? AssetName, string? Error, string ProxyNote);

    public static string StagingDir
    {
        get
        {
            var d = Path.Combine(Path.GetTempPath(), "MDReader-Update");
            Directory.CreateDirectory(d);
            return d;
        }
    }

    // ---- 版本比较 --------------------------------------------------------

    /// <summary>按数字段逐位比，认不出来的段当 0；v 前缀可有可无。</summary>
    public static bool IsNewer(string latest, string current)
    {
        var a = Key(latest); var b = Key(current);
        for (int i = 0; i < Math.Max(a.Length, b.Length); i++)
        {
            var x = i < a.Length ? a[i] : 0;
            var y = i < b.Length ? b[i] : 0;
            if (x != y) return x > y;
        }
        return false;

        static int[] Key(string? v)
        {
            var list = new List<int>();
            foreach (Match m in Regex.Matches((v ?? "").TrimStart('v', 'V'), @"\d+"))
                list.Add(int.TryParse(m.Value, out var n) ? n : 0);
            return list.ToArray();
        }
    }

    // ---- 检查 ------------------------------------------------------------

    public static async Task<CheckResult> CheckAsync()
    {
        var current = typeof(Updater).Assembly.GetName().Version?.ToString(3) ?? "0.0.0";
        var client = NetGuard.NewClient(out var proxyNote);
        try
        {
            var (tag, asset) = await FetchLatestAsync(client).ConfigureAwait(false);
            if (tag == null)
            {
                Log($"检查：没有已发布的版本（{proxyNote}）");
                return new CheckResult(false, current, null, false, null,
                    "这个仓库还没有发布版本（走的是" + proxyNote + "）", proxyNote);
            }
            var available = IsNewer(tag, current);
            Log($"检查：当前 v{current} 最新 v{tag} 需更新={available} 资产={asset?.Name ?? "无"}（{proxyNote}）");
            return new CheckResult(true, current, tag, available, asset?.Name, null, proxyNote);
        }
        catch (Exception e)
        {
            Log($"检查失败：{e.GetType().Name} {e.Message}（{proxyNote}）");
            return new CheckResult(false, current, null, false, null, Friendly(e), proxyNote);
        }
        finally { client.Dispose(); }
    }

    private sealed record Asset(string Name, string Url, long Size);

    /// <summary>API 优先；限流/异常回退 releases.atom（不占 API 配额），再补一次 expanded_assets 抓直链。</summary>
    private static async Task<(string? tag, Asset? asset)> FetchLatestAsync(HttpClient client)
    {
        string? apiError = null;
        try
        {
            using var r = await GetAsync(client, $"https://api.github.com/repos/{Repo}/releases/latest", CheckTimeout)
                .ConfigureAwait(false);
            if (r.StatusCode == HttpStatusCode.NotFound) return (null, null);
            if (r.StatusCode == HttpStatusCode.Forbidden)
                apiError = "GitHub API 限流(403)";
            else if (!r.IsSuccessStatusCode)
                apiError = $"HTTP {(int)r.StatusCode}";
            else
            {
                using var doc = JsonDocument.Parse(await r.Content.ReadAsStringAsync().ConfigureAwait(false));
                var root = doc.RootElement;
                if (root.TryGetProperty("draft", out var d) && d.GetBoolean()) return (null, null);
                var tag = Str(root, "tag_name")?.TrimStart('v', 'V');
                Asset? pick = null;
                if (root.TryGetProperty("assets", out var assets) && assets.ValueKind == JsonValueKind.Array)
                {
                    foreach (var a in assets.EnumerateArray())
                    {
                        var name = Str(a, "name") ?? "";
                        var url = Str(a, "browser_download_url") ?? "";
                        var size = a.TryGetProperty("size", out var s) && s.TryGetInt64(out var sz) ? sz : 0;
                        if (name.ToLowerInvariant().Contains(AssetHint)) { pick = new Asset(name, url, size); break; }
                        pick ??= new Asset(name, url, size);
                    }
                }
                if (tag != null) return (tag, pick);
            }
        }
        catch (Exception e) { apiError = e.Message; }

        // atom 兜底
        using var ar = await GetAsync(client, $"https://github.com/{Repo}/releases.atom", CheckTimeout).ConfigureAwait(false);
        ar.EnsureSuccessStatusCode();
        var body = await ar.Content.ReadAsStringAsync().ConfigureAwait(false);
        var entry = body.IndexOf("<entry>", StringComparison.Ordinal);
        if (entry < 0) return (null, null);
        var link = Regex.Match(body[entry..], @"<link[^>]*href=""(https://github\.com/[^""]*/releases/tag/[^""]+)""");
        if (!link.Success) return (null, null);
        var t = Uri.UnescapeDataString(link.Groups[1].Value.Split("/releases/tag/")[1]).TrimStart('v', 'V');
        // 没有资产直链就去 expanded_assets 页里抓
        try
        {
            using var er = await GetAsync(client,
                $"https://github.com/{Repo}/releases/expanded_assets/{Uri.EscapeDataString(t)}", CheckTimeout).ConfigureAwait(false);
            if (er.IsSuccessStatusCode)
            {
                var html = await er.Content.ReadAsStringAsync().ConfigureAwait(false);
                foreach (Match m in Regex.Matches(html, @"href=""(/[^""]*/releases/download/[^""]+)"""))
                {
                    var url = "https://github.com" + m.Groups[1].Value;
                    var name = Path.GetFileName(m.Groups[1].Value);
                    if (name.ToLowerInvariant().Contains(AssetHint)) return (t, new Asset(name, url, 0));
                }
            }
        }
        catch (Exception e) { Log($"expanded_assets 抓直链失败：{e.GetType().Name} {e.Message}（版本号仍带回去）"); }
        return (t, null);
    }

    private static string? Str(JsonElement e, string p) =>
        e.TryGetProperty(p, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

    /// <summary>传输层抖动重试：最多三次，退避 1.5s / 3s。HTTP 状态码不算失败，不重试。</summary>
    private static async Task<HttpResponseMessage> GetAsync(HttpClient client, string url, TimeSpan timeout)
    {
        Exception? last = null;
        for (var attempt = 1; attempt <= 3; attempt++)
        {
            try { return await GetOnceAsync(client, url, timeout).ConfigureAwait(false); }
            catch (Exception e) when (NetGuard.IsTransient(e) && attempt < 3)
            {
                last = e;
                Log($"第 {attempt} 次出网失败（{e.GetType().Name}: {e.Message}），{(attempt * 1.5):F1}s 后重试");
                await Task.Delay(TimeSpan.FromSeconds(attempt * 1.5)).ConfigureAwait(false);
            }
            catch (Exception e) { last = e; break; }
        }
        throw last!;
    }

    /// <summary>GET 且手工跟随重定向：每一跳都重新过 NetGuard。</summary>
    private static async Task<HttpResponseMessage> GetOnceAsync(HttpClient client, string url, TimeSpan timeout)
    {
        var until = Stopwatch.StartNew();
        var current = url;
        for (var hop = 0; hop <= 5; hop++)
        {
            NetGuard.Validate(current);
            using var req = new HttpRequestMessage(HttpMethod.Get, current);
            if (hop > 0) req.Headers.Remove("Accept");     // 重定向后按 GitHub 页面走，不再声明 API 媒体类型
            var remain = timeout - until.Elapsed;
            if (remain <= TimeSpan.Zero) throw new TaskCanceledException($"超时（>{timeout.TotalSeconds:F0}s）");
            using var cts = new CancellationTokenSource(remain);
            HttpResponseMessage resp;
            try
            {
                resp = await client.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, cts.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                throw new TaskCanceledException($"请求超时（>{timeout.TotalSeconds:F0}s，含域名解析）");
            }
            var code = (int)resp.StatusCode;
            if (code is 301 or 302 or 303 or 307 or 308 && resp.Headers.Location != null)
            {
                var next = resp.RequestMessage?.RequestUri is { } baseUri
                    ? new Uri(baseUri, resp.Headers.Location).ToString() : resp.Headers.Location.ToString();
                resp.Dispose();
                current = next;
                continue;
            }
            return resp;
        }
        throw new Exception("重定向次数过多（>5 跳），已停止");
    }

    /// <summary>
    /// 拿到检查结果后真的把包取下来。资产直链缺失时不猜 URL——直接抛错，
    /// 由界面把发布页地址给人自己点（宁可少做一步，不做"看起来成功了"）。
    /// </summary>
    public static async Task<string> DownloadAndResolveAsync(CheckResult r, IProgress<string> progress)
    {
        var client = NetGuard.NewClient(out _);
        Asset? asset;
        try
        {
            var (tag, found) = await FetchLatestAsync(client).ConfigureAwait(false);
            if (found == null || string.IsNullOrEmpty(found.Url))
                throw new Exception($"v{tag} 这一版没有可下载的更新包，请到发布页手动下载");
            asset = found;
        }
        finally { client.Dispose(); }
        return await DownloadAsync(asset.Url, asset.Size, progress).ConfigureAwait(false);
    }

    // ---- 下载 ------------------------------------------------------------

    public static async Task<string> DownloadAsync(string url, long expectedSize, IProgress<string> progress)
    {
        var name = Path.GetFileName(Uri.UnescapeDataString(NetGuard.Validate(url).AbsolutePath));
        if (string.IsNullOrWhiteSpace(name)) name = "update.zip";
        var dest = Path.Combine(StagingDir, name);
        if (!dest.StartsWith(StagingDir, StringComparison.OrdinalIgnoreCase))
            throw new Exception("下载目标越出暂存目录，已拒绝");

        var errors = new List<string>();
        // 候选源：目前只有 GitHub 一条；镜像接进来后这里变成两条，换源逻辑不用动
        var candidates = new List<string> { url };
        Log($"开始下载 {NetGuard.Validate(url).Host} 预期 {expectedSize} 字节，候选源 {candidates.Count} 个");
        for (int i = 0; i < candidates.Count; i++)
        {
            var last = i == candidates.Count - 1;
            try
            {
                await DownloadOneAsync(candidates[i], dest, expectedSize, progress, last).ConfigureAwait(false);
                var size = new FileInfo(dest).Length;
                if (size < MinPackageBytes) { TryDelete(dest); throw new Exception($"更新包过小（{size} 字节），已丢弃"); }
                if (expectedSize > 0 && size != expectedSize) { TryDelete(dest); throw new Exception($"下载不完整（{size}/{expectedSize} 字节）"); }
                progress.Report("100%");
                Log($"下载完成 {size} 字节 -> {dest}");
                return dest;
            }
            catch (Exception e)
            {
                errors.Add($"{new Uri(candidates[i]).Host}: {e.Message}");
                Log($"源失败：{errors[^1]}");
                CleanupSidecars(dest);
                TryDelete(dest);
            }
        }
        throw new Exception("所有更新源均失败：\n" + string.Join("\n", errors));
    }

    private static async Task DownloadOneAsync(string url, string dest, long expected, IProgress<string> progress, bool lastSource)
    {
        var client = NetGuard.NewClient(out _);
        using (client)
        {
            var (total, ranged) = await ProbeAsync(client, url).ConfigureAwait(false);
            if (total <= 0) total = expected;
            using var cts = new CancellationTokenSource();
            var done = 0L;
            var pump = Task.Run(async () =>
            {
                var watch = Stopwatch.StartNew();
                long mark = 0;
                while (!cts.IsCancellationRequested)
                {
                    await Task.Delay(2000).ConfigureAwait(false);
                    if (watch.Elapsed.TotalSeconds < StallWindowSec) continue;
                    var now = Interlocked.Read(ref done);
                    if (now <= mark) { cts.Cancel(); return; }                    // 停滞：判死
                    if (now - mark < SlowBytes && !lastSource) { cts.Cancel(); return; }  // 龟速：换源
                    mark = now; watch.Restart();
                    progress.Report($"{(total > 0 ? (int)Math.Min(99, now * 100 / total) : 0)}%  {(now / 1048576.0):F1} MB");
                }
            });

            try
            {
                if (ranged && total > 4 * ParallelChunks * 1024 * 1024)
                    await RunChunksAsync(client, url, dest, total, progress, cts.Token).ConfigureAwait(false);
                else
                    await RunSingleAsync(client, url, dest, progress, cts.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                throw new Exception($"下载停滞或过慢（{StallWindowSec} 秒窗口内不足 {SlowBytes / 1048576}MB）");
            }
            finally
            {
                if (!pump.IsCompleted) cts.Cancel();
                try { await pump.ConfigureAwait(false); } catch { /* 看门狗自己退出即可 */ }
            }
            _ = Interlocked.Read(ref done);
        }
    }

    /// <summary>Range 0-0 探测：服务器认不认分块、总大小多少。</summary>
    private static async Task<(long total, bool ranged)> ProbeAsync(HttpClient client, string url)
    {
        using var req = new HttpRequestMessage(HttpMethod.Get, url);
        req.Headers.Range = new RangeHeaderValue(0, 0);
        using var resp = await client.SendAsync(req, HttpCompletionOption.ResponseHeadersRead).ConfigureAwait(false);
        if ((int)resp.StatusCode == 206 && resp.Content.Headers.ContentRange != null)
            return (resp.Content.Headers.ContentRange.Length ?? 0, true);
        return (resp.Content.Headers.ContentLength ?? 0, false);
    }

    private static async Task RunSingleAsync(HttpClient client, string url, string dest, IProgress<string> progress, CancellationToken ct)
    {
        var tmp = dest + ".single.tmp";
        await using (var fs = new FileStream(tmp, FileMode.Create, FileAccess.Write))
            await using (var s = await StreamAsync(client, url, null, ct).ConfigureAwait(false))
            {
                await s.CopyToAsync(fs, 1 << 16, ct).ConfigureAwait(false);
            }
        File.Move(tmp, dest, true);
    }

    private static async Task RunChunksAsync(HttpClient client, string url, string dest, long total, IProgress<string> progress, CancellationToken ct)
    {
        var parts = new string[ParallelChunks];
        var segs = new (long lo, long hi)[ParallelChunks];
        for (var i = 0; i < ParallelChunks; i++)
        {
            segs[i] = (i * total / ParallelChunks, (i + 1) * total / ParallelChunks - 1);
            parts[i] = $"{dest}.part{i}.tmp";
        }
        var tasks = new List<Task>();
        for (var i = 0; i < ParallelChunks; i++)
        {
            var idx = i;
            tasks.Add(Task.Run(async () =>
            {
                await using var fs = new FileStream(parts[idx], FileMode.Create, FileAccess.Write);
                await using var s = await StreamAsync(client, url, segs[idx], ct).ConfigureAwait(false);
                await s.CopyToAsync(fs, 1 << 16, ct).ConfigureAwait(false);
            }, ct));
        }
        await Task.WhenAll(tasks).ConfigureAwait(false);
        foreach (var (p, seg) in parts.Zip(segs))
        {
            var got = new FileInfo(p).Length;
            if (got != seg.hi - seg.lo + 1) throw new Exception($"分块不完整（{got}/{seg.hi - seg.lo + 1}）");
        }
        await using (var outFs = new FileStream(dest, FileMode.Create, FileAccess.Write))
            foreach (var p in parts)
            {
                await using var inFs = new FileStream(p, FileMode.Open, FileAccess.Read);
                await inFs.CopyToAsync(outFs, 1 << 20, CancellationToken.None).ConfigureAwait(false);
            }
        foreach (var p in parts) TryDelete(p);
    }

    private static async Task<Stream> StreamAsync(HttpClient client, string url, (long lo, long hi)? range, CancellationToken ct)
    {
        NetGuard.Validate(url);
        using var req = new HttpRequestMessage(HttpMethod.Get, url);
        if (range is { } r) req.Headers.Range = new RangeHeaderValue(r.lo, r.hi);
        var resp = await client.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
        var code = (int)resp.StatusCode;
        if (code is 301 or 302 or 307 or 308 && resp.Headers.Location != null)
        {
            var next = new Uri(resp.RequestMessage!.RequestUri!, resp.Headers.Location).ToString();
            resp.Dispose();
            return await StreamAsync(client, next, range, ct).ConfigureAwait(false);
        }
        if (!resp.IsSuccessStatusCode)
        {
            var msg = $"HTTP {code}";
            resp.Dispose();
            throw new Exception(msg);
        }
        return await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false);
    }

    // ---- 日志 -------------------------------------------------------------

    /// <summary>
    /// 出问题时得有一处可查——静默检查失败如果只把状态条还原，事后什么都查不到。
    /// 写在 %APPDATA%\MDReader\update.log，纯文本、可直接删，超过 200KB 自己截断。
    /// </summary>
    public static void Log(string line)
    {
        try
        {
            var dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MDReader");
            Directory.CreateDirectory(dir);
            var p = Path.Combine(dir, "update.log");
            if (new FileInfo(p).Exists && new FileInfo(p).Length > 200_000)
                File.WriteAllText(p, "（旧日志已截断）\n");
            File.AppendAllText(p, $"{DateTime.Now:yyyy-MM-dd HH:mm:ss}  {line}\n");
        }
        catch (Exception) { /* 日志写不下不影响功能 */ }
    }

    // ---- 其它 ------------------------------------------------------------

    public static string Friendly(Exception e)
    {
        var m = e.Message ?? "";
        if (m.Contains("超时") || m.Contains("timeout", StringComparison.OrdinalIgnoreCase))
            return "网络超时，请检查代理/网络后重试";
        if (m.Contains("403") || m.Contains("限流", StringComparison.OrdinalIgnoreCase))
            return "GitHub 匿名配额已用完（走代理时常见）；在 %APPDATA%\\MDReader\\github-token.txt 放一个 token 可解决";
        return m;
    }

    /// <summary>token 只从显式位置读：环境变量，或本程序自己目录下的文件。绝不去翻别的工具的凭据。</summary>
    public static string? ReadToken()
    {
        var env = Environment.GetEnvironmentVariable("GH_TOKEN") ?? Environment.GetEnvironmentVariable("GITHUB_TOKEN");
        if (!string.IsNullOrWhiteSpace(env)) return env.Trim();
        try
        {
            var p = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MDReader", "github-token.txt");
            if (File.Exists(p))
            {
                var first = File.ReadAllText(p).Split('\n')[0].Trim();
                if (first.Length > 0) return first;
            }
        }
        catch (Exception) { /* 读不到就当没有，匿名也能查公开仓 */ }
        return null;
    }

    private static void CleanupSidecars(string dest)
    {
        TryDelete(dest + ".single.tmp");
        for (var i = 0; i < ParallelChunks; i++) TryDelete($"{dest}.part{i}.tmp");
    }

    private static void TryDelete(string p)
    {
        try { if (File.Exists(p)) File.Delete(p); } catch (IOException) { /* 被占用就留给下次 */ }
    }
}
