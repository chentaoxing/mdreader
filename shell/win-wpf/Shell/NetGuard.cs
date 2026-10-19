using System.Collections.Generic;
using System.Net;
using System.IO;
using System.Net.Http;
using System.Threading.Tasks;

namespace MdFileAssistant.Shell;

/// <summary>
/// 出网前的那道闸：只允许 https、只允许白名单主机、解析出来的 IP 不能是内网/回环/链路本地/保留段。
/// 重定向**逐跳重新校验**（否则一个 302 就把白名单绕过了）。
/// 这套口径照 ChinaSeal 的 updater.py 搬，那是在真机上跑通过、也被人拿 SSRF 清单挑过刺的版本。
/// </summary>
public static class NetGuard
{
    /// <summary>允许出网的主机。国内镜像（AtomGit / gitcode）接进来时往这里加一条。</summary>
    private static readonly HashSet<string> AllowedHosts = new(StringComparer.OrdinalIgnoreCase)
    {
        "api.github.com", "github.com", "codeload.github.com", "raw.githubusercontent.com",
        "objects.githubusercontent.com", "release-assets.githubusercontent.com", "uploads.github.com",
        // "atomgit.com", "file-cdn.gitcode.com",   // 镜像仓库建好后取消注释
    };

    public static bool IsAllowedHost(string host) => AllowedHosts.Contains(host);

    /// <summary>校验一个 URL；不合法就抛异常（消息要能直接进状态条）。</summary>
    public static Uri Validate(string url)
    {
        if (!Uri.TryCreate(url, UriKind.Absolute, out var u)) throw new Exception("非法 URL：" + Trim(url));
        if (u.Scheme != Uri.UriSchemeHttps) throw new Exception("只允许 https，当前是 " + u.Scheme);
        if (!IsAllowedHost(u.Host)) throw new Exception("主机不在白名单：" + u.Host);
        foreach (var ip in Resolve(u.Host))
            if (IsRestricted(ip)) throw Restricted(u.Host, ip);
        return u;
    }

    /// <summary>
    /// 内网 / 回环 / 链路本地 / 组播 / 保留段一律拒绝——这是防 SSRF 的那道闸：
    /// 白名单被 DNS 指到 127.0.0.1 或 169.254.169.254（元数据服务）时不能放过去。
    /// 自己按字节判，不用 GetAddressTypes（那套 API 在这个目标框架上取不到）。
    /// </summary>
    public static bool IsRestricted(IPAddress ip)
    {
        var b = ip.GetAddressBytes();
        if (b.Length == 4) return IsRestrictedV4(b);
        if (b.Length == 16)
        {
            if (ip.IsIPv6LinkLocal || ip.IsIPv6SiteLocal || ip.IsIPv6Multicast) return true;
            if (b.AsSpan(0, 15).IndexOfAnyExcept((byte)0) < 0 && b[15] == 1) return true;   // ::1 回环
            // ::ffff:a.b.c.d —— 内嵌 IPv4，按 v4 再判一次
            if (b.AsSpan(0, 10).IndexOfAnyExcept((byte)0) < 0 && b[10] == 0xff && b[11] == 0xff)
                return IsRestrictedV4(b[12..]);
            return false;
        }
        return true;   // 看不懂的地址族，不放行
    }

    private static bool IsRestrictedV4(byte[] b) =>
        b[0] == 0 || b[0] == 10 || b[0] == 127 || b[0] >= 224 ||
        (b[0] == 100 && b[1] >= 64 && b[1] <= 127) ||            // CGNAT 100.64/10
        (b[0] == 169 && b[1] == 254) ||                          // 链路本地（含云元数据 169.254.169.254）
        (b[0] == 172 && b[1] >= 16 && b[1] <= 31) ||             // 私有 172.16/12
        (b[0] == 192 && b[1] == 168) ||                          // 私有 192.168/16
        (b[0] == 198 && (b[1] == 18 || b[1] == 19));             // 基准测试保留

    private static Exception Restricted(string host, IPAddress ip) =>
        new Exception($"主机 {host} 解析到受限地址 {ip}，已拒绝出网");

    private static readonly Dictionary<string, IPAddress[]> DnsCache = new();
    private static readonly Dictionary<string, DateTime> DnsAt = new();
    private static readonly TimeSpan DnsTtl = TimeSpan.FromMinutes(5);

    /// <summary>带 5 秒硬超时的解析：DNS 挂住时不能让检查按钮永远转圈。</summary>
    private static IPAddress[] Resolve(string host)
    {
        if (DnsCache.TryGetValue(host, out var hit) && DateTime.UtcNow - DnsAt[host] < DnsTtl) return hit;
        try
        {
            var task = Dns.GetHostAddressesAsync(host);
            if (!task.Wait(TimeSpan.FromSeconds(5))) throw new Exception($"域名解析超时（>5s）：{host}");
            var ips = task.Result;
            if (ips.Length == 0) throw new Exception($"域名解析无结果：{host}");
            DnsCache[host] = ips;
            DnsAt[host] = DateTime.UtcNow;
            return ips;
        }
        catch (AggregateException ae) when (ae.InnerException is not null)
        {
            throw new Exception($"域名解析失败：{host}（{ae.InnerException.Message}）");
        }
    }

    /// <summary>
    /// 造一个走系统代理、且不自动跟随重定向的 HttpClient。
    /// 代理这块是这台机器的雷：v2rayN 设的是系统代理，环境变量与 WinHTTP 各读各的，
    /// 所以这里显式按「环境变量 → 系统代理」的顺序挑一个，并把用的哪一个回显出来。
    /// </summary>
    public static HttpClient NewClient(out string proxyNote)
    {
        var h = new HttpClientHandler
        {
            AllowAutoRedirect = false,          // 逐跳自己校验
            UseCookies = false,
            UseDefaultCredentials = false,
        };
        var env = Environment.GetEnvironmentVariable("HTTPS_PROXY") ?? Environment.GetEnvironmentVariable("https_proxy");
        if (!string.IsNullOrWhiteSpace(env) && Uri.TryCreate(env.Trim(), UriKind.Absolute, out var pu))
        {
            h.Proxy = new WebProxy(pu);
            h.UseProxy = true;
            proxyNote = "代理 " + pu.Host;
        }
        else
        {
            h.Proxy = WebRequest.DefaultWebProxy;   // 系统代理（WinHTTP 解析）
            h.UseProxy = true;
            proxyNote = "系统代理";
        }
        var c = new HttpClient(h) { Timeout = TimeSpan.FromSeconds(60) };
        c.DefaultRequestHeaders.UserAgent.ParseAdd("mdreader-updater/0.1");
        c.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
        return c;
    }

    private static string Trim(string s) => s.Length <= 80 ? s : s[..80] + "…";

    /// <summary>
    /// 这台机器的代理会随机抖：同一段时间里 curl 报 exit 35、git 报 schannel 握手失败、
    /// gh 报 EOF、应用里报 "The SSL connection could not be established"。
    /// 传输层失败（不是 HTTP 状态码）要当成可重试的——否则一次抖动就变成"静默检查没发现更新"，
    /// 用户看到的永远是"没有新版"，既不是事实也无从追查。
    /// </summary>
    public static bool IsTransient(Exception e) =>
        e is HttpRequestException or System.Net.Sockets.SocketException or TaskCanceledException or IOException
        || e.InnerException is HttpRequestException or System.Net.Sockets.SocketException;
}
