/*
 * ffi_c_test.c — mdcore-ffi 的真实 C 客户端验收（补齐 HANDOFF 声称但仓库里没有的东西）
 *
 * 编译：
 *   gcc -std=c11 -I include ffi_c_test.c -L target/release -lmdcore_ffi -Wl,-rpath,$PWD/target/release
 * 运行：
 *   ./a.out all          跑全部非破坏性检查
 *   ./a.out free_empty   单独进程：按 mdfile.h 第 55 行的字面约定释放"无错误"返回值
 *   ./a.out double_free  单独进程：同一指针释放两次
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
#include <stdint.h>

const char *mdfile_engine_version(void);
char *mdfile_last_error(void);
void mdfile_free_string(char *ptr);
int32_t mdfile_optimize(const uint8_t *input, size_t input_len, const char *preset,
                        const char *newline, char **out_json, size_t *out_len);
int32_t mdfile_rules_json(char **out_json, size_t *out_len);
int32_t mdfile_rebuild(const uint8_t *original, size_t original_len, const char *preset,
                       const char *decisions_json, char **out_json, size_t *out_len);

static int fails = 0;
#define CHECK(cond, msg) do { if (cond) { printf("  PASS  %s\n", msg); } else { printf("  FAIL  %s\n", msg); fails++; } } while (0)

/*
 * 从 JSON 里取 "key":"..." 并把 latin1 通道还原成字节。
 * 注意：JSON 文本本身是 UTF-8，所以 rendered_latin1 里的 U+0080~U+00FF 会被编成
 * 两个 UTF-8 字节；必须先把码点解出来再截成字节（任何正经 JSON 解析器都是这么做的，
 * 这里手搓只是为了不引依赖）。
 */
static int extract_field(const char *json, const char *key, char *buf, size_t bufsz, size_t *raw_len)
{
    char pat[128];
    snprintf(pat, sizeof pat, "\"%s\":\"", key);
    const char *p = strstr(json, pat);
    if (!p) return 0;
    p += strlen(pat);
    size_t o = 0;
    while (*p && o + 1 < bufsz) {
        unsigned int cp = 0;
        unsigned char c = (unsigned char)*p;
        if (c == '"') break;
        if (c == '\\') {
            p++;
            switch (*p) {
                case 'n': cp = '\n'; p++; break;
                case 't': cp = '\t'; p++; break;
                case 'r': cp = '\r'; p++; break;
                case 'b': cp = '\b'; p++; break;
                case 'f': cp = '\f'; p++; break;
                case '"': cp = '"'; p++; break;
                case '\\': cp = '\\'; p++; break;
                case '/': cp = '/'; p++; break;
                case 'u': {
                    unsigned int v = 0;
                    if (sscanf(p + 1, "%4x", &v) != 1) return -1;
                    p += 5;
                    if (v >= 0xD800 && v <= 0xDFFF) { fprintf(stderr, "  !! 测试解析器不支持代理对\n"); return -1; }
                    cp = v;
                    break;
                }
                default: cp = (unsigned char)*p; p++; break;
            }
        } else if (c < 0x80) {
            cp = c; p++;
        } else if ((c & 0xE0) == 0xC0) {
            cp = ((unsigned)(c & 0x1F) << 6) | (unsigned char)p[1]; p += 2;
        } else if ((c & 0xF0) == 0xE0) {
            cp = ((unsigned)(c & 0x0F) << 12) | (((unsigned)(unsigned char)p[1] & 0x3F) << 6) | (unsigned char)p[2]; p += 3;
        } else {
            fprintf(stderr, "  !! 出现 U+00FF 以上的码点，latin1 通道不成立\n"); return -1;
        }
        if (cp > 0xFF) { fprintf(stderr, "  !! 码点超出一字节\n"); return -1; }
        buf[o++] = (char)cp;
    }
    buf[o] = 0;
    *raw_len = o;
    return 1;
}

static void test_basic_roundtrip(void)
{
    printf("[1] 基本优化 + rendered_latin1 逐字节还原（BOM/CRLF）\n");
    /* a) 无需改动的文件：必须逐字节相同 */
    const char *clean = "\357\273\277# \346\240\207\351\242\230\r\n\r\n\346\255\243\346\226\207\r\n";
    size_t cn = strlen(clean);
    char *o1 = NULL; size_t l1 = 0;
    int32_t c1 = mdfile_optimize((const uint8_t *)clean, cn, "技术文档", "preserve", &o1, &l1);
    CHECK(c1 == 0, "a) 干净文件状态码 0");
    char buf1[4096]; size_t g1 = 0;
    if (c1 == 0 && o1) {
        int r = extract_field(o1, "rendered_latin1", buf1, sizeof buf1, &g1);
        CHECK(r == 1 && g1 == cn && memcmp(buf1, clean, cn) == 0, "a) 零改动时逐字节还原（含 BOM+CRLF）");
        printf("  info  a) 原 %zu -> 还原 %zu\n", cn, g1);
        mdfile_free_string(o1);
    }
    /* b) 有冗余空行的文件：改动后 BOM 与 CRLF 仍须保留 */
    const char *src = "\357\273\277# \346\240\207\351\242\230\r\n\r\n\r\n\r\n\346\255\243\346\226\207\r\n";
    size_t n = strlen(src);
    char *out = NULL; size_t out_len = 0;
    int32_t code = mdfile_optimize((const uint8_t *)src, n, "技术文档", "preserve", &out, &out_len);
    CHECK(code == 0, "b) 状态码 0");
    if (code != 0) { const char *e = mdfile_last_error(); printf("  code=%d err=%s\n", code, e ? e : "(null)"); return; }
    if (!out) { printf("  FAIL 状态码 0 但没有输出\n"); return; }
    size_t got = 0;
    char buf[4096];
    int r = extract_field(out, "rendered_latin1", buf, sizeof buf, &got);
    CHECK(r == 1, "b) 取到 rendered_latin1");
    CHECK(got < n, "b) 空行被压缩（字节数减少）");
    CHECK(got >= 3 && memcmp(buf, "\357\273\277", 3) == 0, "b) BOM 原样保留");
    CHECK(got >= 2 && memcmp(buf + got - 2, "\r\n", 2) == 0, "b) 结尾 CRLF 保留");
    int crlf = 0;
    for (size_t i = 0; i + 1 < got; i++) if (buf[i] == '\r' && buf[i + 1] == '\n') crlf++;
    printf("  info  b) 原 %zu -> 还原 %zu, CRLF 数=%d\n", n, got, crlf);
    CHECK(crlf >= 3, "b) 全文仍用 CRLF（未被换成 LF）");
    CHECK(strstr(out, "\"content_changes\":0") != NULL, "b) content_changes 为 0");
    CHECK(strstr(out, "\"had_bom\":true") != NULL, "b) had_bom=true");
    printf("  info  out_len=%zu, JSON strlen=%zu（out_len 是 JSON 文本长度，不是文件长度）\n", out_len, strlen(out));
    CHECK(out_len == strlen(out), "b) out_len == JSON 文本长度");
    mdfile_free_string(out);
}

static void test_nul_byte(void)
{
    printf("[2] 文件里含 0x00 字节时能否逐字节还原\n");
    unsigned char src[64];
    size_t i = 0;
    src[i++] = '#'; src[i++] = ' '; src[i++] = 'T'; src[i++] = '\n';
    src[i++] = '\n'; src[i++] = 'a'; src[i++] = 0x00; src[i++] = 'b'; src[i++] = '\n';
    char *out = NULL; size_t out_len = 0;
    int32_t code = mdfile_optimize(src, i, NULL, NULL, &out, &out_len);
    CHECK(code == 0, "含 NUL 的文件仍返回 0");
    if (code != 0) { printf("  code=%d\n", code); return; }
    char buf[4096]; size_t got = 0;
    int r = extract_field(out, "rendered_latin1", buf, sizeof buf, &got);
    printf("  info  原 %zu 字节 -> 还原 %zu 字节, parse=%d\n", i, got, r);
    CHECK(r == 1 && got == i && memcmp(buf, src, i) == 0, "NUL 文件逐字节还原");
    mdfile_free_string(out);
}

static void test_oversize_and_empty(void)
{
    printf("[3] 边界参数\n");
    char *out = NULL; size_t l = 0;
    int32_t c1 = mdfile_optimize(NULL, 0, NULL, NULL, &out, &l);
    printf("  info  (NULL,0) code=%d\n", c1);
    CHECK(c1 == 0, "(NULL,0) 视为空文件 -> 0（malloc(0) 可能返回 NULL）");
    if (out) mdfile_free_string(out);
    out = NULL; l = 0;
    int32_t c0 = mdfile_optimize(NULL, 4, NULL, NULL, &out, &l);
    CHECK(c0 == -1, "(NULL,4) 仍是非法参数 -> -1");
    out = NULL; l = 0;
    int32_t c2 = mdfile_optimize((const uint8_t *)"", 0, NULL, NULL, &out, &l);
    printf("  info  (有效指针,0) code=%d  <- 同一个空文件两种结果不一致\n", c2);
    if (out) mdfile_free_string(out);
    out = NULL; l = 0;
    unsigned char *huge = (unsigned char *)calloc(64 * 1024 * 1024 + 2, 1);
    huge[0] = '#'; huge[1] = ' '; huge[2] = 'X';
    int32_t c3 = mdfile_optimize(huge, 64 * 1024 * 1024 + 2, NULL, NULL, &out, &l);
    CHECK(c3 == 2, "超过 64MiB 上限 -> 2");
    printf("  info  oversize code=%d\n", c3);
    free(huge);
    if (out) mdfile_free_string(out);
    out = NULL; l = 0;
    CHECK(mdfile_optimize((const uint8_t *)"\xFF\xFE\x41", 3, NULL, NULL, &out, &l) == 1, "非法 UTF-8 -> 1");
    out = NULL; l = 0;
    CHECK(mdfile_optimize((const uint8_t *)"# A\n", 4, "不存在的预设", NULL, &out, &l) == 4, "未知预设 -> 4");
    out = NULL; l = 0;
    CHECK(mdfile_optimize((const uint8_t *)"# A\n", 4, NULL, NULL, NULL, NULL) == -1, "输出指针为 NULL -> -1");
}

static void test_stale_error(void)
{
    printf("[4] LAST_ERROR 是否在成功后被清除\n");
    char *out = NULL; size_t l = 0;
    mdfile_optimize((const uint8_t *)"# A\n", 4, "不存在的预设", NULL, &out, &l);
    const char *e1 = mdfile_last_error();
    int had1 = e1 && e1[0] != '\0';
    printf("  info  失败调用后 last_error=\"%s\"\n", e1 ? e1 : "(null)");
    out = NULL; l = 0;
    int32_t ok = mdfile_optimize((const uint8_t *)"# A\n", 4, NULL, NULL, &out, &l);
    const char *e2 = mdfile_last_error();
    printf("  info  成功调用后 last_error=\"%s\"\n", e2 ? e2 : "(null)");
    CHECK(had1 && ok == 0 && e2 && e2[0] == '\0', "成功后错误消息应被清空");
    if (had1 && e1 && e1[0]) mdfile_free_string((char *)e1);
    if (e2 && e2[0]) mdfile_free_string((char *)e2);
    if (out) mdfile_free_string(out);
}

static void test_rebuild_claims(void)
{
    printf("[5] mdfile_rebuild 的 content_changes 是算出来的还是写死的\n");
    const char *src = "# \346\240\207\351\242\230   \n\n\n\n\346\255\243\346\226\207\n";
    const char *dec = "[{\"decision\":\"reject\",\"rule_id\":\"R02.trailing-whitespace\"}]";
    char *out = NULL; size_t l = 0;
    int32_t code = mdfile_rebuild((const uint8_t *)src, strlen(src), NULL, dec, &out, &l);
    CHECK(code == 0, "rebuild 返回 0");
    if (code != 0 || !out) { const char *e = mdfile_last_error(); printf("  info  code=%d err=%s\n", code, e ? e : "(null)"); return; }
    printf("  info  reply=%s\n", out);
    CHECK(strstr(out, "\"content_changes\"") != NULL, "rebuild 返回 content_changes 字段");
    CHECK(strstr(out, "\"rendered_latin1\"") != NULL, "rebuild 也返回格式层渲染结果（与 optimize 一致）");
    mdfile_free_string(out);
    /* CRLF 输入走 rebuild：与 optimize 不同入口是否一致 */
    const char *crlf = "# \346\240\207\351\242\230   \r\n\r\n\r\n\r\n\346\255\243\346\226\207\r\n";
    out = NULL; l = 0;
    int32_t c2 = mdfile_rebuild((const uint8_t *)crlf, strlen(crlf), NULL, "[]", &out, &l);
    printf("  info  rebuild(CRLF) code=%d len=%zu\n", c2, out ? strlen(out) : 0);
    if (c2 == 0 && out) { printf("  info  body=%s\n", strstr(out, "\"body\":") ? strstr(out, "\"body\":") : "?"); mdfile_free_string(out); }
    /* 同样的 CRLF 输入走 optimize，比较两者结论 */
    char *o3 = NULL; size_t l3 = 0;
    int32_t c3 = mdfile_optimize((const uint8_t *)crlf, strlen(crlf), NULL, NULL, &o3, &l3);
    printf("  info  optimize(CRLF) code=%d\n", c3);
    if (c3 == 0 && o3) { printf("  info  optimize 结论片段=%s\n", strstr(o3, "\"structural_changes\":") ); mdfile_free_string(o3); }
}

static void test_u64_precision(void)
{
    printf("[6] checksum 字段在 JSON 里的位数（Web/JNI 侧是否会被截断）\n");
    size_t cap = 600000;
    unsigned char *big = (unsigned char *)calloc(cap, 1);
    int n = 0;
    n += sprintf((char *)big + n, "# T\n\n");
    for (int k = 0; k < 40000 && n + 32 < (int)cap; k++) n += sprintf((char *)big + n, "token%d\n\n", k);
    char *out = NULL; size_t l = 0;
    int32_t code = mdfile_optimize(big, (size_t)n, NULL, NULL, &out, &l);
    CHECK(code == 0, "40k token 输入成功");
    const char *p = out ? strstr(out, "\"checksum_original\":") : NULL;
    if (p) { p += strlen("\"checksum_original\":"); printf("  info  checksum_original=%s (17 位十进制 > 2^53，JS JSON.parse 会丢精度)\n", p); }
    free(big);
    if (out) mdfile_free_string(out);
}

int main(int argc, char **argv)
{
    setbuf(stdout, NULL);   /* 崩溃时也不要丢日志 */
    const char *mode = argc > 1 ? argv[1] : "all";
    printf("engine=%s\n", mdfile_engine_version());
    if (strcmp(mode, "free_empty") == 0) {
        const char *p = mdfile_last_error();
        printf("free_empty: ptr=%p first_byte=%d\n", (void *)p, p ? (unsigned char)p[0] : -1);
        if (p) { mdfile_free_string((char *)p); printf("free 未崩溃（说明该指针是堆上的）\n"); }
        return 0;
    }
    if (strcmp(mode, "double_free") == 0) {
        char *out = NULL; size_t l = 0;
        mdfile_optimize((const uint8_t *)"# A\n\n\n\nb\n", 9, NULL, NULL, &out, &l);
        mdfile_free_string(out);
        mdfile_free_string(out);
        printf("double free 未崩溃\n");
        return 0;
    }
    if (strcmp(mode, "t1") == 0) { test_basic_roundtrip(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    if (strcmp(mode, "t2") == 0) { test_nul_byte(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    if (strcmp(mode, "t3") == 0) { test_oversize_and_empty(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    if (strcmp(mode, "t4") == 0) { test_stale_error(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    if (strcmp(mode, "t5") == 0) { test_rebuild_claims(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    if (strcmp(mode, "t6") == 0) { test_u64_precision(); printf("FAIL=%d\n", fails); return fails ? 1 : 0; }
    char *o = NULL; size_t l = 0;
    if (mdfile_rules_json(&o, &l) == 0) { printf("rules_json OK (%zu bytes)\n", l); mdfile_free_string(o); }
    test_basic_roundtrip();
    test_nul_byte();
    test_oversize_and_empty();
    test_stale_error();
    test_rebuild_claims();
    test_u64_precision();
    printf("\n结果：FAIL=%d\n", fails);
    return fails ? 1 : 0;
}
