# Frontend i18n (zh / en) — Design

**Date:** 2026-04-18
**Status:** approved (brainstorm phase)
**Scope:** frontend only (`frontend/`) — no Rust backend changes.

## Problem

The desktop/web frontend ships a single vanilla HTML/JS SPA with English UI labels hardcoded in `index.html` and ~1,600 lines of render functions in `frontend/js/app.js`. `<html lang="zh-CN">` is set but all visible text is English. Users who read Chinese have no way to switch, and users who prefer English are served a mislabelled document. We need a runtime zh/en switcher that feels native on both sides.

## Goals

- Runtime switchable Chinese (zh) / English (en) UI, no page reload required.
- Initial language follows `navigator.language` (zh\* → zh, else en); user's manual choice persists in `localStorage` and overrides browser default on future visits.
- Language switcher available in two places: header top-right (one-click toggle) and Settings page (discoverable section).
- Zero build step — stays consistent with the existing no-framework, no-bundler frontend.
- Keep translation strings grouped and editable by a non-programmer (flat key → string map per locale).

## Non-goals

- **ECharts chart internals** (titles, legends, axis names, toolbox tooltips) — stay in English.
- **Dynamic runtime strings**: progress text (`"Running... 42%"`), data-driven table headers, file-count messages — stay in English.
- **Backend-originated text**: `RunEvent::Log` lines streamed from subprocesses, `validate()` error messages from Rust adapters — stay in English. Translating these would require pattern-mapping or Rust changes, and risks translation drift from the tool's real output.
- **Rust workspace**: no changes to any crate. This is pure frontend work.
- **Third-party locale packages** (i18next, vue-i18n, etc.): not used. The tiny custom module below is enough.
- **Language-aware units/dates**: not introduced; no existing code formats dates or numbers for locale.

## Architecture

### File layout

```
frontend/
├── index.html              # static strings annotated with data-i18n; switcher button added
├── js/
│   ├── i18n.js             # NEW — dictionary + t() / setLang() / applyI18n()
│   └── app.js              # MODIFIED — render fns use I18N.t(), Settings adds Language section
└── css/style.css           # MODIFIED — .lang-toggle styles
```

`i18n.js` loads before `app.js` via a new `<script src="js/i18n.js"></script>` tag in `index.html`.

### `i18n.js` public API

Exposed as `window.I18N`:

```js
window.I18N = {
  t(key, vars)        // 'nav.dashboard' → localised string; vars (optional) is {name: 'X'} for {name} placeholders
  setLang(lang)       // 'zh' | 'en'; writes localStorage, updates <html lang>, calls applyI18n(document), dispatches 'langchange' event on window
  getLang()           // returns 'zh' | 'en'
  applyI18n(root)     // scans root for [data-i18n] and [data-i18n-attr], rewrites textContent / attributes
};
```

Implementation outline (no dependencies, < 100 lines):

- Dictionary is an inline object literal `{ zh: {...}, en: {...} }` with nested keys.
- `t(key, vars)` walks the key path (`'nav.qc'` → `dict[lang].nav.qc`). If missing, falls back to `dict.en[key]`, then the key string itself. Placeholders `{name}` replaced via `vars`.
- Initial language resolved at script load:
  1. `localStorage.getItem('rustbrain.lang')` if one of `'zh' | 'en'`
  2. else `navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'`
- `applyI18n(root)` selectors:
  - `[data-i18n]` → set `el.textContent = t(el.dataset.i18n)`
  - `[data-i18n-attr]` → value is `"attr:key[,attr:key]*"`, e.g. `"placeholder:common.search_files"`; for each pair, `el.setAttribute(attr, t(key))`
- On DOM ready, `applyI18n(document)` runs once so initial HTML gets translated before the user sees it.

### HTML annotation convention

Static text nodes get `data-i18n="<key>"`:

```html
<span class="breadcrumb-home" data-i18n="brand.name">RustBrain</span>
<span data-i18n="nav.dashboard">Dashboard</span>
<span class="nav-badge ready" data-i18n="badge.ready">Ready</span>
```

The English fallback stays in the HTML — this keeps the source file readable and gives graceful degradation if `i18n.js` fails to load.

Attributes use `data-i18n-attr`:

```html
<input data-i18n-attr="placeholder:common.search_files" placeholder="Search files">
```

### `app.js` integration

Render functions currently build HTML via template literals with inline strings. We add `const t = (k, v) => I18N.t(k, v);` inside each render function (or at the top of the IIFE) and replace literal strings that are UI chrome:

```js
// before
return `<button class="btn btn-primary">Run Analysis</button>`;

// after
return `<button class="btn btn-primary">${t('common.run')}</button>`;
```

In-scope strings (see §Translation coverage); out-of-scope strings stay as-is.

**Re-render on language change.** Because render functions produce HTML strings, we can't mutate in place — we re-trigger the router:

```js
window.addEventListener('langchange', () => navigate(state.currentView));
```

For static parts of `index.html` (sidebar, header chrome, status bar) that live outside `#content`, `applyI18n(document)` updates them directly via the `data-i18n` attributes — no re-render needed.

### Language switcher UI

**Header button** (in `index.html`, inside `.header-right`, to the left of `.project-selector`):

```html
<div class="lang-toggle" role="group" aria-label="Language">
  <button class="lang-btn" data-lang="en">EN</button>
  <button class="lang-btn" data-lang="zh">中文</button>
</div>
```

Active language gets `.active` class (styled in `style.css`). Click handler attached once in `app.js` init:

```js
document.querySelectorAll('.lang-btn').forEach(btn => {
  btn.addEventListener('click', () => I18N.setLang(btn.dataset.lang));
});
window.addEventListener('langchange', () => {
  document.querySelectorAll('.lang-btn').forEach(b =>
    b.classList.toggle('active', b.dataset.lang === I18N.getLang())
  );
});
```

**Settings page** adds a new section rendered by `renderSettings()`:

```
┌─ Language / 语言 ─────────────────┐
│ ( ) English                       │
│ (•) 中文                          │
└───────────────────────────────────┘
```

Changing the radio calls `I18N.setLang(value)`. Because the header toggle and Settings radio both listen to `langchange`, they stay in sync automatically.

### Startup flow

1. `<script src="js/i18n.js">` runs first, resolves initial language, sets `window.I18N` and `<html lang>`.
2. `<script src="js/app.js">` runs, defines render functions that call `I18N.t()`.
3. `DOMContentLoaded` fires → `app.js` calls `I18N.applyI18n(document)` on chrome, then `navigate(initialView)` renders first view using `t()`.
4. Lucide icons initialised (unchanged).

## Translation coverage (Q3 = A: UI chrome only)

**In scope** — strings that must be translated:

- Sidebar: section titles, nav item labels, "Ready"/"Soon" badges, "Backend Connected", brand sub-title
- Header: breadcrumb labels (static portion — the dynamic view name is derived from `nav.*`), "No Project" placeholder
- Status bar: "Ready", "No active jobs", "Rust" tag
- Button labels in render functions: "Run", "Cancel", "Save", "Close", "Browse", "Select Files", "Select Directory"
- Empty-state messages: "No files selected", "Loading…"
- Form labels and section headings inside module views (`renderQC`, `renderTrimming`, `renderStarIndex`, `renderStarAlign`, `renderDifferential`, `renderNetwork`, `renderSettings`, `renderComingSoon`)
- Dashboard: section titles, quick-action card titles and descriptions, tool-info card labels
- Settings: nav items ("Binary Paths", the new "Language"), button labels

**Out of scope** — stays English:

- All ECharts chart content (title / legend / xAxis / yAxis / toolbox.feature.*.title)
- Progress text built from `RunEvent::Progress` (percent / stage names from backend)
- Streamed log lines from `RunEvent::Log`
- `validate()` error strings bubbled up from Rust
- Table cell contents (data-driven)
- Console.log / developer-only messages

### Dictionary structure

```js
const DICT = {
  zh: {
    brand: { name: 'RustBrain', subtitle: '转录组分析平台' },
    nav: {
      overview: '总览',
      pipeline: '分析流程',
      alignment_quant: '比对与定量',
      system: '系统',
      dashboard: '仪表盘',
      qc: '质量控制',
      trimming: '接头修剪',
      alignment: '序列比对',
      quantification: '定量',
      differential: '差异表达',
      network: '网络分析',
      enrichment: '功能富集',
      star_index: 'STAR 索引',
      star_align: 'STAR 比对',
      settings: '设置',
    },
    badge: { ready: '就绪', soon: '即将推出' },
    status: {
      ready: '就绪',
      no_jobs: '无活动任务',
      backend_connected: '后端已连接',
      rust_tag: 'Rust',
    },
    project: { no_project: '未选择项目', new: '新建项目', open: '打开项目' },
    common: {
      run: '运行', cancel: '取消', save: '保存', close: '关闭',
      browse: '浏览', select_files: '选择文件', select_dir: '选择目录',
      empty_no_files: '尚未选择文件', loading: '加载中…',
    },
    settings: {
      title: '设置',
      binary_paths: '二进制路径',
      language_section: '语言 / Language',
      language_en: 'English',
      language_zh: '中文',
    },
    module: {
      qc: { title: '质量控制', /* per-view keys added during implementation */ },
      trimming: { title: '接头修剪' },
      star_index: { title: 'STAR 索引' },
      star_align: { title: 'STAR 比对' },
      differential: { title: '差异表达分析' },
      network: { title: '网络分析' },
      coming_soon: { title: '即将推出', body: '此模块正在开发中。' },
    },
  },
  en: { /* mirror, English original strings */ }
};
```

The exact leaf set for `module.*` is fleshed out during implementation — the plan enumerates strings view-by-view so nothing is missed. Leaf keys added on the zh side must also appear on the en side (and vice versa).

## Error handling

- Missing key: `t()` returns the key itself (e.g. `'nav.dashboard'`). Visible in the UI = fast signal during development. No throw.
- Missing language in localStorage or corrupted value: detected by `!['zh','en'].includes(...)`, falls through to `navigator.language` default.
- `i18n.js` fails to load (404, syntax error): `app.js` guards by `const t = window.I18N?.t || ((k) => k);`. Untranslated English originals in HTML `data-i18n` fallback text remain visible — UI is degraded but usable.

## Testing strategy

No existing frontend test infrastructure; testing is manual against the Python dev server:

```bash
cd frontend && python3 -m http.server 8090
```

Manual checklist executed during implementation:

1. First load with no localStorage — browser lang `en` → English; browser lang `zh-CN` → Chinese.
2. Click header toggle EN → zh → verify sidebar, header, status bar, current view all switch without reload.
3. Click toggle again zh → EN → same verification.
4. Open Settings → Language section radio reflects current language; changing it updates header toggle too.
5. Refresh page → stored language persists.
6. Navigate to every view (dashboard, qc, trimming, star-index, star-align, differential, network, enrichment, settings) in both languages — visually scan for any English leakage (in-scope strings) or broken Chinese (out-of-scope strings correctly left in English).
7. Run a module (mock backend) — verify progress text / log lines / chart labels stay English as designed.
8. Verify `<html lang>` attribute updates with language choice (browser-level a11y signal).

## Risks & open questions

- **Key drift between locales**: mitigated by a tiny dev-only console assertion at the bottom of `i18n.js` that walks both dictionaries and logs any asymmetric keys. No build-time check.
- **Chinese font rendering**: the page already loads Google Fonts (Zilla Slab, Karla, Fira Code) — none cover CJK. Browsers will fall back to system CJK font (fine on Windows/macOS/most Linux distros). If the fallback looks poor, the implementation plan may add `font-family` fallbacks like `'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', sans-serif`. Flagged for implementation-time visual review.
- **Line length / layout**: Chinese translations are often shorter character-wise but wider per character; English "Differential Expr." → "差异表达分析" fits the sidebar width, but badge labels ("Ready" → "就绪" is narrower; "Soon" → "即将推出" is wider) may need CSS width adjustment. Verified during manual test step 6.

## Out of scope for this spec (future work)

- Translating chart axes/legends if user feedback asks for it (would require per-chart config pass-through).
- Translating backend log/error text (requires a Rust-side strategy — error codes vs strings).
- Additional languages (ja, es, ...): the structure supports them but none are requested.
