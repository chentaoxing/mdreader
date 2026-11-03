package com.chentaoxing.mdreader

import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.chentaoxing.mdreader.core.*
import com.chentaoxing.mdreader.render.ReadingView
import com.chentaoxing.mdreader.render.SymbolMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val initialUri: Uri? = intent?.takeIf { it.action == android.content.Intent.ACTION_VIEW }?.data
        setContent { MDReaderApp(initialUri) }
    }
}

private enum class Tab(val label: String) { Reading("阅读"), Changes("改动"), Score("评分") }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MDReaderApp(initialUri: Uri?) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()

    // 文档状态
    var uri by remember { mutableStateOf<Uri?>(initialUri) }
    var displayName by remember { mutableStateOf("") }
    var coreBytes by remember { mutableStateOf(ByteArray(0)) }
    var encLabel by remember { mutableStateOf("") }
    var transcoded by remember { mutableStateOf(false) }

    // 核心结果
    var presets by remember { mutableStateOf(listOf("通用文章", "技术文档", "AI 生成文档", "小说")) }
    var preset by remember { mutableStateOf(RecentStore.preset(ctx)) }
    var result by remember { mutableStateOf<OptimizeResult?>(null) }
    var rebuildResult by remember { mutableStateOf<RebuildResult?>(null) }
    var accept by remember { mutableStateOf(mapOf<String, Boolean>()) }
    var statusLine by remember { mutableStateOf("") }
    var tab by remember { mutableStateOf(Tab.Reading) }
    var presetMenu by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }

    // 显示偏好（记忆）+ 最近列表
    var symbolMode by remember { mutableStateOf(RecentStore.symbolMode(ctx)) }
    var showRecents by remember { mutableStateOf(false) }
    var recents by remember { mutableStateOf(RecentStore.recents(ctx)) }

    LaunchedEffect(Unit) {
        runCatching {
            val env = Envelope.parse(MdCore.rulesJson())
            if (env is Envelope.Ok) presets = Wire.presets(env.payload)
        }
        statusLine = "引擎 ${MdCore.engineVersion} · 只调排版、不改正文 · 全程离线"
    }

    fun runOptimize(bytes: ByteArray, name: String) {
        scope.launch {
            busy = true
            val r = withContext(Dispatchers.Default) { Envelope.parse(MdCore.optimize(bytes, preset, "preserve")) }
            when (r) {
                is Envelope.Err -> {
                    result = null
                    statusLine = "✗ ${Wire.statusText(r.status)}${if (r.error.isNotEmpty()) "：${r.error}" else ""}"
                }
                is Envelope.Ok -> {
                    val o = Wire.optimize(r.payload)
                    result = o
                    rebuildResult = null
                    accept = o.applied.associate { it.ruleId to true }
                    statusLine = if (o.contentUntouched)
                        "✓ 正文内容变化 0 · 结构变化 ${o.structuralChanges} 处 · 校验和前后一致" +
                            (if (transcoded) " · 源文件是 $encLabel，现在读的是内存里的 UTF-8 副本" else "")
                    else "✗ 正文内容变化非 0，不允许保存"
                }
            }
            busy = false
        }
    }

    fun runRebuild() {
        val cur = result ?: return
        scope.launch {
            busy = true
            val decisions = JSONArray().apply {
                cur.applied.forEach { a ->
                    if (accept[a.ruleId] != false) put(JSONObject().put("decision", "accept").put("rule_id", a.ruleId))
                }
                cur.applied.forEach { a ->
                    if (accept[a.ruleId] == false) put(JSONObject().put("decision", "reject").put("rule_id", a.ruleId))
                }
            }
            val r = withContext(Dispatchers.Default) {
                Envelope.parse(MdCore.rebuild(coreBytes, preset, decisions.toString()))
            }
            when (r) {
                is Envelope.Err -> statusLine = "✗ 重建失败 ${Wire.statusText(r.status)}：${r.error}"
                is Envelope.Ok -> {
                    val rb = Wire.rebuild(r.payload)
                    if (rb.contentChanges != 0 || rb.checksumStr != cur.checksumOriginalStr) {
                        statusLine = "✗ 重建后校验和与原文不一致，不采用"
                    } else {
                        rebuildResult = rb
                        statusLine = "✓ 已按你的勾选重建 · 正文内容变化 0"
                    }
                }
            }
            busy = false
        }
    }

    fun loadUri(u: Uri) {
        scope.launch {
            busy = true
            try {
                val raw = withContext(Dispatchers.IO) { ctx.contentResolver.openInputStream(u)!!.use { it.readBytes() } }
                val name = queryDisplayName(ctx, u)
                val dec = TextDecode.decode(raw)
                if (dec == null) {
                    uri = u; displayName = name
                    coreBytes = ByteArray(0); result = null; rebuildResult = null
                    statusLine = "✗ 既不是 UTF-8 也不是 GB18030/UTF-16 —— 认不出的编码我不猜，文件也没动"
                } else {
                    uri = u; displayName = name
                    coreBytes = dec.utf8; encLabel = dec.label; transcoded = dec.transcoded
                    // SAF URI 的读权限默认只到本进程结束：持久化后「最近列表」重开才有效
                    runCatching {
                        ctx.contentResolver.takePersistableUriPermission(
                            u, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }
                    RecentStore.add(ctx, u.toString(), name)
                    recents = RecentStore.recents(ctx)
                    runOptimize(dec.utf8, name)
                }
            } catch (e: Exception) {
                statusLine = "读取失败：${e.message}"
            }
            busy = false
        }
    }

    val openLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { u: Uri? ->
        u?.let { loadUri(it) }
    }
    val saveLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/markdown")) { u: Uri? ->
        if (u != null) {
            val bytes = rebuildResult?.rendered ?: result?.rendered
            if (bytes != null) {
                scope.launch {
                    runCatching {
                        withContext(Dispatchers.IO) {
                            ctx.contentResolver.openOutputStream(u, "wt")!!.use { it.write(bytes) }
                        }
                    }.onSuccess {
                        statusLine = "✓ 已写出优化版（原文件未动，正文变化 0）"
                    }.onFailure { statusLine = "写出失败：${it.message}" }
                }
            }
        }
    }

    LaunchedEffect(initialUri) { initialUri?.let { loadUri(it) } }

    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) {
            if (android.os.Build.VERSION.SDK_INT >= 31) dynamicDarkColorScheme(ctx) else darkColorScheme()
        } else {
            if (android.os.Build.VERSION.SDK_INT >= 31) dynamicLightColorScheme(ctx) else lightColorScheme()
        }
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Column {
                        Text(if (displayName.isEmpty()) "MD文档阅读助手" else displayName,
                            style = MaterialTheme.typography.titleMedium, maxLines = 1)
                        Text("引擎 ${MdCore.engineVersion} · 永不写回原文件",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant)
                    } },
                    actions = {
                        // 语法符号三档：原样 / 弱化 / 隐藏（产品招牌：把 ** 从屏幕上拿掉）
                        TextButton(onClick = {
                            symbolMode = when (symbolMode) { SymbolMode.Raw -> SymbolMode.Dim; SymbolMode.Dim -> SymbolMode.Hidden; SymbolMode.Hidden -> SymbolMode.Raw }
                            RecentStore.setSymbolMode(ctx, symbolMode)
                        }) { Text("Aa·${symbolMode.label}", style = MaterialTheme.typography.labelLarge) }
                        TextButton(onClick = { recents = RecentStore.recents(ctx); showRecents = true }) {
                            Text("最近", style = MaterialTheme.typography.labelLarge)
                        }
                        Box {
                            TextButton(onClick = { presetMenu = true }) { Text(preset) }
                            DropdownMenu(expanded = presetMenu, onDismissRequest = { presetMenu = false }) {
                                presets.forEach { p ->
                                    DropdownMenuItem(text = { Text(p) }, onClick = {
                                        preset = p; presetMenu = false
                                        RecentStore.setPreset(ctx, p)
                                        if (coreBytes.isNotEmpty()) runOptimize(coreBytes, displayName)
                                    })
                                }
                            }
                        }
                        TextButton(onClick = {
                            if (result?.contentUntouched == true || rebuildResult != null) {
                                saveLauncher.launch("${displayName.removeSuffix(".md")}_阅读优化版.md")
                            }
                        }) { Text("另存") }
                        TextButton(onClick = {
                            openLauncher.launch(arrayOf("text/markdown", "text/x-markdown", "text/plain", "application/octet-stream"))
                        }) { Text("打开") }
                    },
                )
            },
        ) { pad ->
            Column(Modifier.padding(pad).fillMaxSize()) {
                // 状态条
                Surface(color = MaterialTheme.colorScheme.surfaceVariant, modifier = Modifier.fillMaxWidth()) {
                    Text(
                        statusLine,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
                if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())

                if (result == null) {
                    EmptyHint()
                    return@Column
                }

                // 页签
                TabRow(selectedTabIndex = tab.ordinal) {
                    Tab.entries.forEach { t ->
                        Tab(selected = tab == t, onClick = { tab = t },
                            text = { Text(t.label) })
                    }
                }

                val body = rebuildResult?.body ?: result!!.body
                when (tab) {
                    Tab.Reading -> ReadingView(body, Modifier.fillMaxSize(), symbolMode)
                    Tab.Changes -> ChangesTab(result!!, accept,
                        { id, v -> accept = accept + (id to v); runRebuild() })
                    Tab.Score -> ScoreTab(result!!)
                }
            }

            if (showRecents) {
                ModalBottomSheet(onDismissRequest = { showRecents = false }) {
                    LazyColumn(Modifier.padding(bottom = 24.dp)) {
                        item {
                            Text("最近打开",
                                style = MaterialTheme.typography.titleMedium,
                                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
                        }
                        if (recents.isEmpty()) {
                            item {
                                Text("还没有记录。打开一份 .md 后会出现在这里，点一下即可重读。",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
                            }
                        }
                        items(recents, key = { it.uri }) { e ->
                            ListItem(
                                headlineContent = { Text(e.name, maxLines = 1) },
                                supportingContent = { Text(
                                    java.text.SimpleDateFormat("MM-dd HH:mm", java.util.Locale.getDefault())
                                        .format(java.util.Date(e.time)),
                                    style = MaterialTheme.typography.labelSmall) },
                                trailingContent = {
                                    TextButton(onClick = {
                                        RecentStore.remove(ctx, e.uri)
                                        recents = RecentStore.recents(ctx)
                                    }) { Text("移除", style = MaterialTheme.typography.labelSmall) }
                                },
                                modifier = Modifier.clickable {
                                    showRecents = false
                                    loadUri(android.net.Uri.parse(e.uri))
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyHint() {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("打开一份 .md 开始读", style = MaterialTheme.typography.titleLarge)
        Spacer(Modifier.height(8.dp))
        Text(
            "把它当「阅读器」用：左上角「打开」选文件，或从文件管理器直接分享过来。\n" +
                "只调排版、一个字的内容都不改；另存永远是新文件，原文件不会被动。",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun ChangesTab(
    r: OptimizeResult,
    accept: Map<String, Boolean>,
    onToggle: (String, Boolean) -> Unit,
) {
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp)) {
        if (r.applied.isEmpty() && r.rejected.isEmpty()) {
            item { Text("这份文件无需结构改动", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        items(r.applied, key = { it.ruleId }) { a ->
            Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = accept[a.ruleId] != false, onCheckedChange = { onToggle(a.ruleId, it) })
                Column(Modifier.weight(1f)) {
                    Text("${a.ruleId} ${a.ruleName}", style = MaterialTheme.typography.bodyMedium)
                    Text(a.reason, style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Text("${a.edits} 处", style = MaterialTheme.typography.labelMedium)
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.surfaceVariant)
        }
        if (r.rejected.isNotEmpty()) {
            item {
                Text("被拒绝的规则（核心已拒绝，不会落盘）",
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(top = 16.dp, bottom = 4.dp))
            }
            items(r.rejected, key = { it.ruleId }) { j ->
                Column(Modifier.padding(vertical = 4.dp)) {
                    Text("${j.ruleId} ${j.ruleName}", style = MaterialTheme.typography.bodyMedium)
                    Text(j.reason, style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable
private fun ScoreTab(r: OptimizeResult) {
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp)) {
        item {
            Row(verticalAlignment = Alignment.Bottom) {
                Text("${r.scoreOverall}", fontSize = 44.sp)
                Text(" / 100", style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(bottom = 8.dp))
                Spacer(Modifier.width(12.dp))
                Text("优化前 ${r.scoreBeforeOverall}", style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(bottom = 8.dp))
            }
            Spacer(Modifier.height(8.dp))
            Text("评分是提示，不构成质量保证；分数上升不代表「更好读」的主观判断成立。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(12.dp))
            Text("正文 token 校验和（原）：${r.checksumOriginalStr}",
                style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp))
            Text("正文 token 校验和（新）：${r.checksumOptimizedStr}",
                style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp))
            Text("收敛轮次：${r.passes} · 换行风格：${r.newlineStyle} · BOM：${if (r.hadBom) "有" else "无"}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

private fun queryDisplayName(ctx: Context, u: Uri): String {
    return try {
        ctx.contentResolver.query(u, null, null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                val idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (idx >= 0) c.getString(idx) ?: "document.md" else "document.md"
            } else "document.md"
        } ?: "document.md"
    } catch (e: Exception) {
        "document.md"
    }
}
