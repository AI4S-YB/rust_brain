# Frontend Modularization Design

**Date**: 2026-04-19
**Status**: Approved
**Scope**: `frontend/` only — no backend, no Tauri command changes

## Problem

`frontend/js/app.js` is 1,982 lines in a single IIFE containing ~50 functions covering routing, seven module views, submit handlers, chart renderers, settings, file-drop handling, log streaming, and init wiring. `frontend/css/style.css` is 1,896 lines; `frontend/js/i18n.js` is 699 lines (mostly dictionaries). `frontend/index.html` embeds a 37-line Tauri browser-mode mock in a `<script>` tag.

The code works, but adding a new analysis module requires editing `app.js` near the top (config), the middle (render + submit + charts), and the bottom (events/init). There is no dependency graph — everything lives on a shared IIFE closure and two `window.*` globals (`window.I18N`, `window.exportTableAsTSV`).

## Goals

1. Each file has one clear purpose; adding a new module means adding one file under `js/modules/` plus one line in `router.js`.
2. Explicit dependencies via ES module `import` — no implicit `window.*` coupling (except `window.__TAURI__` which is injected by the host).
3. Zero build step, matching the existing `CLAUDE.md` constraint. Continue to serve from `python3 -m http.server` for dev preview and the Tauri dev shell for integration.
4. Behavior-preserving: no visual changes, no API changes, no new features.

## Non-Goals

- Introducing TypeScript, a bundler (Vite/esbuild), or a framework.
- Replacing ECharts or Lucide CDN `<script>` with ESM imports.
- CSS minification, combining, or variables restructuring.
- Any Tauri command, Rust code, or backend change.

## Approach

Native ES modules (`<script type="module">`), medium granularity (per-module files), shared CSS split via `@import`, i18n dictionaries split by language.

Rejected alternatives:
- **IIFE + `window.RB.*` namespaces**: preserves "zero build" but doesn't solve implicit load order or global pollution; just renames the globals.
- **Vite/esbuild bundler**: overkill; violates the "no build step" convention in `CLAUDE.md`.

## Target Directory Layout

```
frontend/
├── index.html
├── favicon.svg
├── css/
│   ├── style.css          # @import chain only
│   ├── base.css           # vars + reset + scrollbar + dot-grid bg
│   ├── layout.css         # sidebar / header / content / status-bar
│   ├── components.css     # cards / buttons / forms / tabs / tables / badges / log / modal
│   ├── dashboard.css      # pipeline flow / stats / quick actions
│   ├── modules.css        # module panels / chart container / settings
│   └── animations.css
└── js/
    ├── main.js            # entry: init + Tauri event listeners + boot routing
    ├── config.js          # MODULES, COLOR_MAP, KNOWN_VIEWS, ECHART_THEME
    ├── state.js           # shared mutable state (currentView, files, pipelineStatus, …)
    ├── tauri-api.js       # invoke / listen wrapper
    ├── utils.js           # escapeHtml, fmtSize, exportTableAsTSV, promptModal
    ├── router.js          # navigate() + breadcrumb + hashchange + coming-soon/empty
    ├── events.js          # global delegation (click / drag / drop / mobile toggle)
    ├── charts.js          # createChart + ECharts theme + shared chart helpers
    ├── ui-shared.js       # file-drop, run-log panel, runs list, collectModuleParams, renderRunResultHtml
    ├── dashboard.js       # renderDashboard + renderQuickAction + renderToolInfo
    ├── settings.js        # renderSettings + binary path management
    ├── i18n/
    │   ├── index.js       # t / setLang / applyI18n + dev asymmetry check
    │   ├── en.js          # English dictionary (default export)
    │   └── zh.js          # Chinese dictionary (default export)
    ├── modules/
    │   ├── qc.js          # renderQC + renderQCCharts
    │   ├── trimming.js    # renderTrimming + renderTrimmingCharts
    │   ├── star-index.js  # renderStarIndex + submitStarIndex
    │   ├── star-align.js  # renderStarAlign + submitStarAlign + renderStarAlignResult + renderMappingRateChart
    │   ├── gff-convert.js # renderGffConvert + submitGffConvert + renderGffConvertResult
    │   ├── differential.js# renderDifferential + renderDESeq2Charts + renderCustomPlotPanel
    │   └── network.js     # renderNetwork + renderWGCNACharts
    └── dev/
        └── mock-tauri.js  # browser-mode Tauri shim (no-op when window.__TAURI__ exists)
```

**Counts**: 11 top-level JS + 7 module files + 3 i18n files + 1 dev file = 22 JS files. 7 CSS files.

## Module Boundaries & Dependency Graph

Strict layering, lower layers cannot import from higher:

```
Layer 0 (leaves):
  config.js        ← pure constants
  state.js         ← mutable object, no deps
  tauri-api.js     ← reads window.__TAURI__ only
  i18n/*           ← self-contained
  utils.js         ← depends on i18n (promptModal labels are localized)

Layer 1 (capabilities):
  charts.js        → config (ECHART_THEME), utils (escapeHtml)
  ui-shared.js     → state, tauri-api, utils, i18n

Layer 2 (views):
  dashboard.js     → config, state, i18n, utils
  settings.js      → tauri-api, utils, i18n
  modules/*.js     → config, state, tauri-api, utils, charts, ui-shared, i18n

Layer 3 (assembly):
  router.js        → state, config, i18n + static imports of dashboard/settings/modules/*
  events.js        → state, router, ui-shared
  main.js          → router, events, i18n, tauri-api
```

**Rules**:
- `modules/*.js` files must not import each other. Cross-module coordination goes through `router.js`.
- `router.js` uses static `import` for all views (simple, predictable). Dynamic `import()` is left for a future need.
- `state.js` is the single source of mutable app state. Other modules must not define their own top-level `const state = {…}`.
- `charts.js` exports `createChart(container)` and theme helpers only; per-module chart content lives in the module file.

**Export style**: named exports everywhere (`export function …`). Exception: `i18n/en.js` and `i18n/zh.js` use `export default` for the dictionary object.

**`window.*` cleanup**:
- `window.I18N` → **removed**. Replace call sites with `import { t } from './i18n/index.js'`.
- `window.exportTableAsTSV` → **removed**. HTML `onclick="exportTableAsTSV('foo', 'bar.tsv')"` attributes become `data-action="export-tsv" data-table-id="foo" data-filename="bar.tsv"`, handled by event delegation in `events.js`.
- `window.__TAURI__` → kept (injected by Tauri host; out of our control).
- **Kept as compatibility shims** (HTML `onclick` handlers depend on them; converting every one is out of scope for this refactor): `window.projectNew`, `window.projectOpen`, `window.toggleCollapsible`, `window.resetForm`, `window.runModule`, `window.renderCustomPlot`. Each is assigned at the end of the owning ESM file, e.g. `window.toggleCollapsible = toggleCollapsible;`. A follow-up refactor may convert these to `data-action` too.

## index.html Changes

Old:
```html
<script> /* 37-line Tauri mock */ </script>
...
<script src="js/i18n.js"></script>
<script src="js/app.js"></script>
```

New:
```html
<link rel="stylesheet" href="css/style.css">
<script src="https://cdn.jsdelivr.net/npm/echarts@5/..."></script>
<script src="https://unpkg.com/lucide@0.344.0/..."></script>
<script type="module" src="js/dev/mock-tauri.js"></script>
<script type="module" src="js/main.js"></script>
```

`mock-tauri.js` begins with `if (window.__TAURI__) { /* no-op */ } else { window.__TAURI__ = { … } }`. It is safe to load in production (Tauri injects first), so no conditional loading needed.

CSP check: `crates/rb-app/tauri.conf.json` currently allows `script-src 'self' 'unsafe-inline' https://unpkg.com https://cdn.jsdelivr.net`. `'self'` covers `tauri://` local scripts, so ESM will load without CSP changes.

## Migration Steps

Each step is a standalone commit/PR. After each step, both `python3 -m http.server 8090` preview and `cargo tauri dev` must still work.

1. **CSS split** — extract six subfiles from `style.css`, leaving `@import` chain only. No JS changes. Verify visual parity against main.
2. **Extract mock-tauri** — move the inline `<script>` block from `index.html` to `js/dev/mock-tauri.js`. Replace with `<script type="module" src=…>`. Behavior unchanged.
3. **i18n ESM migration** — create `js/i18n/index.js` + `en.js` + `zh.js`. Keep a temporary `window.I18N = { t, setLang, applyI18n, getLang }` assignment in `index.js` so `app.js` continues to work unchanged. Delete old `js/i18n.js`.
4. **app.js core split** — create `config`, `state`, `tauri-api`, `utils`, `charts`, `ui-shared`, `router`, `events`, `main`, `dashboard`, `settings`. Migrate sections one at a time; after each, the app still runs because remaining code in `app.js` imports from the new modules. `app.js` progressively shrinks.
5. **modules/ split** — move `renderQC`, `renderTrimming`, STAR Index, STAR Align, GFF Convert, Differential, Network (plus their submit/charts/result helpers) into `modules/*.js`. `router.js` imports them. Delete `app.js`.
6. **Global cleanup** — remove the `window.I18N` compatibility assignment. Convert every `onclick="exportTableAsTSV(…)"` in HTML-generating template strings to `data-action="export-tsv"` attributes; wire up in `events.js`. Remove `window.exportTableAsTSV`.

## Verification (run after every step)

**Browser preview** (`cd frontend && python3 -m http.server 8090`, open `http://localhost:8090`):
- Dashboard renders with pipeline flow + stats.
- Each view navigates: QC, Trimming, STAR Index, STAR Align, Differential, Network, GFF Convert, Settings.
- Charts render on module tabs (mock data is fine).
- EN/中文 language toggle swaps all labels.
- File drop zone accepts drag/drop and click-to-browse.
- Export TSV button on Differential results table produces a download.

**Tauri smoke** (`cd crates/rb-app && cargo tauri dev`):
- Same navigation pass.
- Trigger at least one `run_module` invocation (STAR Index or GFF Convert); run-log panel streams output.

## Risks

- **ESM requires HTTP**: `file://` double-click won't work. Both dev paths (`python -m http.server` and Tauri) already use HTTP, so no change — but documenting it so nobody tries to open `index.html` directly.
- **Strict mode change**: ESM is implicit strict mode. Existing code is already `'use strict';`, so no semantic drift expected. `this` at top level becomes `undefined` instead of `window` — grep confirms no such usage.
- **Circular imports during migration**: if `app.js` still exists alongside partial ESM files, keep the temporary shim globals (`window.I18N`) until step 6. Never `import` from the old `app.js`.
- **CDN scripts remain classic**: ECharts and Lucide are loaded as non-module scripts; they attach to `window`. Module code reads `window.echarts` / `window.lucide` — accepted as external globals.

## Out of Scope

- TypeScript migration.
- Replacing ECharts/Lucide CDN with ESM modules.
- CSS preprocessor, minification, or combining.
- Tauri command / Rust backend changes.
- Accessibility audit, test framework, or lint config changes.

## Open Questions

None at design time. All clarifying questions resolved in brainstorming conversation.
