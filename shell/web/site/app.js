// MD文档阅读助手 Web 版前端。
// 铁律与其它平台一致：排版判定一行不在这里——要改什么、能不能改，全部以核心 JSON 为准。
// 这一层只做两件事：字节解码（显示层转码）与屏幕摆放（阅读视图）。

import init, { optimize, analyze, rebuild, rules_json, engine_version } from './pkg/mdcore_wasm.js';

// ── 状态 ─────────────────────────────────────────────────────────────
const $ = (id) => document.getElementById(id);
const state = {
  bytes: null,
  name: '',
  result: null,
  rebuildResult: null,
  accept: {},
  mode: 'Dim', // Raw | Dim | Hidden（弱化档是默认：产品招牌体验）
  preset: '通用文章',
  encLabel: '',
  transcoded: false,
};

// ── 显示层解码（与 Windows/Android 同一套顺序）──────────────────────
function decodeBytes(raw) {
  if (raw.includes(0)) return { err: 'NUL' };
  const h = raw.subarray(0, 3);
  if (h[0] === 0xEF && h[1] === 0xBB && h[2] === 0xBF)
    return { utf8: raw.subarray(3), label: 'UTF-8（有 BOM）', transcoded: false };
  if (h[0] === 0xFF && h[1] === 0xFE)
    return { utf8: bytesOf(new TextDecoder('utf-16le').decode(raw.subarray(2))), label: 'UTF-16LE', transcoded: true };
  if (h[0] === 0xFE && h[1] === 0xFF)
    return { utf8: bytesOf(new TextDecoder('utf-16be').decode(raw.subarray(2))), label: 'UTF-16BE', transcoded: true };
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(raw);
    return { utf8: raw, label: 'UTF-8', transcoded: false };
  } catch { /* 落到 GB */ }
  try {
    return { utf8: bytesOf(new TextDecoder('gb18030').decode(raw)), label: 'GB18030（含 GBK，按内容猜的编码，若乱码请人工确认）', transcoded: true };
  } catch { return { err: 'unknown' }; }
}
const bytesOf = (text) => new TextEncoder().encode(text);

// ── WASM 装载 ────────────────────────────────────────────────────────
(async () => {
  await init();
  $('engineLine').textContent = `引擎 ${engine_version()} · 只调排版、一个字的内容都不改 · 全程离线`;
  try {
    const r = JSON.parse(rules_json());
    if (r.status === 0) {
      const sel = $('preset');
      sel.innerHTML = '';
      r.payload.presets.forEach((p) => {
        const o = document.createElement('option');
        o.value = p; o.textContent = p;
        sel.appendChild(o);
      });
    }
  } catch { /* 预设列表加载失败不阻塞阅读 */ }
  say('打开一份 .md 开始读：点「打开」或把文件拖进页面。全程离线，文件不上传。');
  addIntro();
})();

function addIntro() {
  const div = document.createElement('div');
  div.id = 'intro';
  div.innerHTML = `
    <h2>打开一份 .md 开始读</h2>
    <p>今天大量的 .md 是写给 AI 读的：超长并列句、成片的空行、<b>星号满天</b>。
    这个工具按人的阅读习惯重排——<b>一个字的内容都不改</b>。</p>
    <p>把文件拖进来即可；结果可下载为 <code>*_阅读优化版.md</code>，原文件不会被动。</p>
    <p>喜欢它的话，同一份核心还有
      <a href="https://github.com/chentaoxing/mdreader/releases/download/v0.1.4/mdreader-0.1.4-portable.zip">Windows 版</a>、
      <a href="https://github.com/chentaoxing/mdreader/releases/download/v0.1.4/mdreader-0.1.4-android.apk">Android 版</a> 和
      <a href="https://github.com/chentaoxing/mdreader/releases">命令行</a>——
      五个平台对同一份文件逐字节给出同一结论。</p>
    <div class="btns"><button class="primary big" id="introOpen">打开文件</button></div>`;
  document.body.appendChild(div);
  $('introOpen').onclick = () => $('fileInput').click();
}

// ── 打开文件 ─────────────────────────────────────────────────────────
$('btnOpen').onclick = () => $('fileInput').click();
$('fileInput').onchange = (e) => { if (e.target.files[0]) openFile(e.target.files[0]); e.target.value = ''; };

let dragDepth = 0;
const dragHasFiles = (e) => [...(e.dataTransfer?.types || [])].includes('Files');
const hideDropzone = () => { dragDepth = 0; $('dropzone').hidden = true; };
document.addEventListener('dragenter', (e) => {
  if (!dragHasFiles(e)) return;          // 视口变化等怪事件不带 Files，不计数（实测 resize 会让计数失衡卡遮罩）
  dragDepth++; $('dropzone').hidden = false;
});
document.addEventListener('dragleave', (e) => {
  if (!dragHasFiles(e)) return;
  if (--dragDepth <= 0) hideDropzone();
});
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => {
  e.preventDefault(); hideDropzone();
  const f = e.dataTransfer.files[0];
  if (f) openFile(f);
});
window.addEventListener('dragend', hideDropzone);
window.addEventListener('blur', hideDropzone);

async function openFile(file) {
  const raw = new Uint8Array(await file.arrayBuffer());
  const dec = decodeBytes(raw);
  if (dec.err === 'NUL') { say('✗ 文件里含 NUL 字节（多半是无 BOM 的 UTF-16），认不出编码我不猜。', 'bad'); return; }
  if (dec.err === 'unknown') { say('✗ 既不是 UTF-8 也不是 GB18030/UTF-16——认不出的编码我不猜，文件也没动。', 'bad'); return; }
  state.bytes = dec.utf8;
  state.name = file.name;
  state.encLabel = dec.label;
  state.transcoded = dec.transcoded;
  $('intro')?.remove();
  runOptimize();
}

// ── 核心调用 ─────────────────────────────────────────────────────────
function envelope(raw) {
  const o = JSON.parse(raw);
  return o.status === 0 ? { ok: true, payload: o.payload } : { ok: false, status: o.status, error: o.error };
}
function statusText(code) {
  return {
    1: '输入不是合法 UTF-8（核心绝不猜编码）', 2: '输入超过 64 MiB 上限',
    3: 'Content Guard 拒绝：正文发生变化，结果已丢弃', 4: '未知预设/规则/参数', 5: '核心内部异常',
  }[code] || `未知状态 ${code}`;
}

function runOptimize() {
  const r = envelope(optimize(state.bytes, state.preset, 'preserve'));
  if (!r.ok) {
    state.result = null;
    $('main').hidden = true; $('btnDownload').disabled = true;
    say(`✗ ${statusText(r.status)}：${r.error}`, 'bad');
    return;
  }
  state.result = r.payload;
  state.rebuildResult = null;
  state.accept = {};
  r.payload.applied.forEach((a) => { state.accept[a.rule_id] = true; });
  render();
  const unchanged = r.payload.content_changes === 0 &&
    r.payload.checksum_original_str === r.payload.checksum_optimized_str;
  say(unchanged
    ? `✓ 正文内容变化 0 · 结构变化 ${r.payload.structural_changes} 处 · 校验和前后一致` +
      (state.transcoded ? ` · 源文件是 ${state.encLabel}，现在读的是内存里的 UTF-8 副本` : '')
    : `✗ 正文内容变化 ${r.payload.content_changes} 处——不允许下载`, unchanged ? 'ok' : 'bad');
}

function runRebuild() {
  const decisions = state.result.applied
    .filter((a) => state.accept[a.rule_id] !== undefined)
    .map((a) => ({ decision: state.accept[a.rule_id] ? 'accept' : 'reject', rule_id: a.rule_id }));
  const r = envelope(rebuild(state.bytes, state.preset, JSON.stringify(decisions)));
  if (!r.ok) { say(`✗ 重建失败 ${statusText(r.status)}：${r.error}`, 'bad'); return; }
  if (r.payload.checksum_str !== state.result.checksum_original_str) {
    say('✗ 重建后校验和与原文不一致，不采用', 'bad'); return;
  }
  state.rebuildResult = r.payload;
  render();
  say('✓ 已按你的勾选重建 · 正文内容变化 0', 'ok');
}

// ── 渲染 ─────────────────────────────────────────────────────────────
function render() {
  $('main').hidden = false;
  $('btnDownload').disabled = !(state.result?.content_changes === 0);
  renderReading();
  renderChanges();
  renderScore();
}

function renderReading() {
  const body = state.rebuildResult?.body ?? state.result.body;
  $('reading').innerHTML = blocksToHtml(parseBlocks(body));
}

// ── 块级解析（移植 Android 壳 ReadingView.parseBlocks；只管屏幕摆放）──
function parseBlocks(body) {
  const out = [];
  const lines = body.split('\n');
  let i = 0;

  if (lines.length && (lines[0] === '---' || lines[0] === '+++')) {
    const fence = lines[0];
    let j = 1; const meta = [];
    while (j < lines.length && lines[j] !== fence) { meta.push(lines[j]); j++; }
    if (j < lines.length) { out.push({ t: 'meta', lines: meta }); i = j + 1; }
  }

  let para = [];
  const flushPara = () => { if (para.length) { out.push({ t: 'para', lines: para }); para = []; } };

  while (i < lines.length) {
    const l = lines[i];
    const trimmed = l.replace(/^ +/, '');
    const indent = l.length - trimmed.length;

    if (!l.trim()) { flushPara(); }
    else if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
      flushPara();
      const fence = trimmed.slice(0, 3);
      const lang = trimmed.slice(3).trim();
      const code = [];
      let j = i + 1;
      while (j < lines.length && !lines[j].trimStart().startsWith(fence)) { code.push(lines[j]); j++; }
      out.push({ t: 'code', lang, lines: code });
      i = j;
    } else if (/^#{1,6} /.test(trimmed) || /^#{1,6}$/.test(trimmed)) {
      flushPara();
      const m = trimmed.match(/^(#{1,6})\s*(.*?)(\s+#+)?\s*$/);
      out.push({ t: 'head', level: m[1].length, text: m[2] });
    } else if (/^(-{3,}|\*{3,}|_{3,})\s*$/.test(trimmed)) {
      flushPara(); out.push({ t: 'rule' });
    } else if (trimmed.startsWith('>')) {
      flushPara();
      const depth = (trimmed.match(/^>+/) || ['>'])[0].length;
      out.push({ t: 'quote', depth, text: trimmed.replace(/^>+ ?/, '') });
    } else if (/^[-*+] /.test(trimmed) || /^\d+\. /.test(trimmed)) {
      flushPara();
      const rest = trimmed.replace(/^[-*+] |^\d+\. /, '');
      const task = rest.match(/^\[([ xX])\] ?(.*)$/);
      out.push({ t: 'item', indent, task: task ? task[1] : null, text: task ? task[2] : rest });
    } else if (trimmed.includes('|') && i + 1 < lines.length && isDelimiter(lines[i + 1].trim())) {
      flushPara();
      const header = splitRow(trimmed);
      const rows = [];
      let j = i + 2;
      while (j < lines.length && lines[j].includes('|') && lines[j].trim()) { rows.push(splitRow(lines[j].trim())); j++; }
      out.push({ t: 'table', header, rows });
      i = j - 1;
    } else {
      para.push(trimmed);
    }
    i++;
  }
  flushPara();
  return out;
}

const isDelimiter = (t) => t.includes('|') && t.includes('-') && t.replace(/[|\-: ]/g, '') === '';
function splitRow(t) {
  let s = t.trim();
  if (s.startsWith('|')) s = s.slice(1);
  if (s.endsWith('|')) s = s.slice(0, -1);
  return s.split('|').map((c) => c.trim());
}

// ── 行内三档渲染（移植 InlineRenderer；原样/弱化/隐藏）──────────────
const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(text) {
  if (state.mode === 'Raw') return esc(text);
  const out = [];
  const push = (s, cls) => out.push(cls ? `<span class="${cls}">${esc(s)}</span>` : esc(s));

  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '`') {
      const close = text.indexOf('`', i + 1);
      if (close > i + 1) {
        out.push(`<code class="inline">${esc(text.slice(i + 1, close))}</code>`);
        i = close + 1; continue;
      }
      push(c); i++; continue;
    }
    if (c === '*') {
      const isBold = text.startsWith('**', i);
      const marker = isBold ? 2 : 1;
      const close = text.indexOf(isBold ? '**' : '*', i + marker);
      if (close > 0) {
        const inner = text.slice(i + marker, close);
        if (state.mode === 'Dim') {
          push(isBold ? '**' : '*', 'sym');
          out.push(isBold ? `<b>${esc(inner)}</b>` : `<i>${esc(inner)}</i>`);
          push(isBold ? '**' : '*', 'sym');
        } else {
          out.push(isBold ? `<b>${esc(inner)}</b>` : `<i>${esc(inner)}</i>`);
        }
        i = close + marker; continue;
      }
      push(c); i++; continue;
    }
    if (c === '[') {
      const close = text.indexOf(']', i);
      const op = close > 0 && text[close + 1] === '(' ? close + 1 : -1;
      const cp = op > 0 ? text.indexOf(')', op) : -1;
      if (cp > op + 1) {
        const label = text.slice(i + 1, close);
        const url = text.slice(op + 1, cp);
        out.push(`<a>${esc(label)}</a>`);
        if (state.mode === 'Dim') push(`(${url})`, 'sym');
        i = cp + 1; continue;
      }
      push(c); i++; continue;
    }
    let j = i;
    while (j < text.length && !'`*['.includes(text[j])) j++;
    push(text.slice(i, j), '');
    i = j;
  }
  return out.join('');
}

// ── 块 → HTML（连续 item 包 <ul>，保证合法结构）─────────────────────
function blocksToHtml(blocks) {
  const html = [];
  let listOpen = false;
  for (const b of blocks) {
    if (b.t !== 'item' && listOpen) { html.push('</ul>'); listOpen = false; }
    switch (b.t) {
      case 'meta':
        html.push(`<div class="meta"><span class="cap">元信息（front matter）</span><br>${b.lines.map(esc).join('<br>')}</div>`);
        break;
      case 'head': {
        const h = `h${Math.min(b.level + 1, 6)}`;
        html.push(`<${h}>${inline(b.text)}</${h}>`);
        break;
      }
      case 'para': html.push(`<p>${inline(b.lines.join(''))}</p>`); break;
      case 'quote':
        html.push(`<blockquote style="--depth:${b.depth - 1}">${inline(b.text)}</blockquote>`);
        break;
      case 'item': {
        if (!listOpen) { html.push('<ul>'); listOpen = true; }
        const done = b.task && b.task !== ' ';
        const bullet = b.task === null ? '•' : b.task === ' ' ? '☐' : '☑';
        html.push(`<li class="${done ? 'task-done' : ''}" style="margin-left:${b.indent * 6}px">` +
          `<span class="bullet">${bullet}</span><span>${inline(b.text)}</span></li>`);
        break;
      }
      case 'code':
        html.push(`<pre>${b.lang ? `<span class="lang">${esc(b.lang)}</span>` : ''}${esc(b.lines.join('\n'))}</pre>`);
        break;
      case 'rule': html.push('<hr>'); break;
      case 'table': {
        const th = b.header.map((c) => `<th>${inline(c)}</th>`).join('');
        const tr = b.rows.map((r) =>
          `<tr>${b.header.map((_, ci) => `<td>${inline(r[ci] ?? '')}</td>`).join('')}</tr>`).join('');
        html.push(`<table><thead><tr>${th}</tr></thead><tbody>${tr}</tbody></table>`);
        break;
      }
    }
  }
  if (listOpen) html.push('</ul>');
  return html.join('');
}

// ── 改动 / 评分 / 下载 ────────────────────────────────────────────────
function renderChanges() {
  const r = state.result;
  const rows = r.applied.map((a) => `
    <div class="rule">
      <input type="checkbox" data-rule="${esc(a.rule_id)}" ${state.accept[a.rule_id] !== false ? 'checked' : ''}>
      <div class="body">
        <span class="id">${esc(a.rule_id)}</span> ${esc(a.rule_name)}
        <div class="reason">${esc(a.reason)}</div>
      </div>
      <span class="count">${a.edits} 处</span>
    </div>`).join('');
  const rejected = r.rejected.length
    ? `<p class="changes-cap">被拒绝的规则（核心已拒绝，不会落盘）</p>` +
      r.rejected.map((j) => `
        <div class="rule rejected">
          <div class="body"><span class="id">${esc(j.rule_id)}</span> ${esc(j.rule_name)}
          <div class="reason">${esc(j.reason)}</div></div>
        </div>`).join('')
    : '';
  $('changes').innerHTML =
    `<p class="changes-cap">勾选决定哪些规则生效（勾选变化即时重建，正文内容恒为 0 变化）</p>${rows}${rejected}`;
  $('changes').querySelectorAll('input[type=checkbox]').forEach((cb) => {
    cb.onchange = () => { state.accept[cb.dataset.rule] = cb.checked; runRebuild(); };
  });
}

function renderScore() {
  const r = state.result;
  const row = (name, before, after) => `<tr><th>${name}</th><td>${before}</td><td>${after}</td></tr>`;
  const s0 = r.score_before, s1 = r.score_after;
  $('score').innerHTML = `
    <div class="score-big"><span class="n">${s1.overall}</span><span>/ 100</span>
      <span class="prev">优化前 ${s0.overall}</span></div>
    <table class="score">
      <tr><th>维度</th><th>优化前</th><th>优化后</th></tr>
      ${row('标题结构', s0.heading, s1.heading)}
      ${row('段落结构', s0.paragraph, s1.paragraph)}
      ${row('列表结构', s0.list, s1.list)}
      ${row('空白分布', s0.whitespace, s1.whitespace)}
      ${row('信息密度', s0.density, s1.density)}
    </table>
    <div class="checksums">
      正文 token 校验和（原）${r.checksum_original_str}<br>
      正文 token 校验和（新）${r.checksum_optimized_str}<br>
      收敛轮次 ${r.passes} · 换行 ${r.newline_style} · ${r.had_bom ? '有 BOM' : '无 BOM'}
    </div>
    <p class="changes-cap">评分是提示，不构成质量保证；分数上升不代表「更好读」这一主观判断成立。</p>`;
}

$('btnMode').onclick = () => {
  state.mode = state.mode === 'Raw' ? 'Dim' : state.mode === 'Dim' ? 'Hidden' : 'Raw';
  $('btnMode').textContent = `Aa·${{ Raw: '原样', Dim: '弱化', Hidden: '隐藏' }[state.mode]}`;
  const body = state.rebuildResult?.body ?? state.result?.body;
  if (body) renderReading();
};

$('btnDownload').onclick = () => {
  const r = state.rebuildResult ?? state.result;
  if (!r) return;
  const bytes = new Uint8Array(r.rendered_latin1.length);
  for (let i = 0; i < r.rendered_latin1.length; i++) bytes[i] = r.rendered_latin1.charCodeAt(i) & 0xFF;
  const blob = new Blob([bytes], { type: 'text/markdown' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = state.name.replace(/\.(md|markdown)$/i, '') + '_阅读优化版.md';
  a.click();
  URL.revokeObjectURL(a.href);
};

$('preset').onchange = () => {
  state.preset = $('preset').value;
  if (state.bytes) runOptimize();
};

document.querySelectorAll('.tab').forEach((t) => {
  t.onclick = () => {
    document.querySelectorAll('.tab').forEach((x) => x.classList.remove('active'));
    document.querySelectorAll('.panel').forEach((x) => x.classList.remove('active'));
    t.classList.add('active');
    $(`panel-${t.dataset.tab}`).classList.add('active');
  };
});

function say(msg, cls) {
  const el = $('status');
  el.textContent = msg;
  el.className = `status ${cls || ''}`;
}

// 自检口（也供自动化测试用）：不是隐藏 API，页面上没有引用它的地方
window.__mdreader = { state, openFile, parseBlocks, inline, runOptimize };
