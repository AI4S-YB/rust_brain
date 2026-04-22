// Plots — a frontend-only visualization playground.
// User pastes or uploads a table, picks a chart type and parameters,
// renders an interactive ECharts chart, and exports PNG/SVG/TSV.
//
// Registry-driven: each chart type lives in charts/{id}.js and exports
// { meta, sample, defaults, renderParams, build }. See charts/common.js.

import { t } from '../../core/i18n-helpers.js';
import { renderModuleHeader } from '../module-header.js';
import { parseTable } from './data.js';
import { CHART_REGISTRY, findChart } from './charts/index.js';
import { exportPng, exportSvg, exportTsv } from './export.js';

const local = {
  chartInstance: null,
  chartType: CHART_REGISTRY[0].meta.id,
  table: null,
  lastOption: null,
  params: {},       // id → user-captured params
  resizeObs: null,
  resizeHandler: null,
};

export function renderPlotsView(container) {
  const mod = {
    id: 'plots', icon: 'line-chart', color: 'gold',
    tool: 'ECharts 5', status: 'ready', name: t('plots.title'),
  };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderBody()}</div>`;
  bind(container);
  resetWith(local.chartType, { loadSample: true });
}

function renderBody() {
  const typeButtons = CHART_REGISTRY.map(c => `
    <button type="button" class="plot-type-btn" data-plot-type="${c.meta.id}">
      <i data-lucide="${c.meta.icon}"></i>
      <span>${t(c.meta.label_key)}</span>
    </button>
  `).join('');

  return `
    <div class="module-layout plots-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:80ms">
          <div class="panel-header"><span class="panel-title">${t('plots.chart_type')}</span></div>
          <div class="panel-body">
            <div class="plot-type-grid">${typeButtons}</div>
          </div>
        </div>

        <div class="module-panel animate-slide-up" style="animation-delay:140ms">
          <div class="panel-header">
            <span class="panel-title">${t('plots.data_input')}</span>
            <div class="plot-data-tools">
              <button type="button" class="btn btn-secondary btn-xs" data-plot-act="load-sample">
                <i data-lucide="sparkles"></i> ${t('plots.load_sample')}
              </button>
              <label class="btn btn-secondary btn-xs plot-upload">
                <i data-lucide="upload"></i> ${t('plots.upload_file')}
                <input type="file" accept=".tsv,.csv,.txt" hidden id="plots-file-input">
              </label>
            </div>
          </div>
          <div class="panel-body">
            <p class="form-hint" id="plots-data-hint">${t('plots.data_hint')}</p>
            <textarea id="plots-data" class="form-input plot-data-area" rows="9"
              placeholder="${t('plots.data_placeholder')}"></textarea>
            <div class="plot-data-summary" id="plots-data-summary"></div>
          </div>
        </div>

        <div class="module-panel animate-slide-up" style="animation-delay:200ms">
          <div class="panel-header"><span class="panel-title">${t('plots.parameters')}</span></div>
          <div class="panel-body">
            <div id="plots-params"></div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-plot-act="reset">
              <i data-lucide="rotate-ccw"></i> ${t('common.reset')}
            </button>
            <button type="button" class="btn btn-primary btn-sm" data-plot-act="render"
              style="background:var(--mod-gold);border-color:var(--mod-gold)">
              <i data-lucide="play"></i> ${t('plots.render')}
            </button>
          </div>
        </div>
      </div>

      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:120ms">
          <div class="panel-header">
            <span class="panel-title" id="plots-preview-title">${t('plots.preview')}</span>
            <div class="plot-export-tools">
              <button type="button" class="btn btn-secondary btn-xs" data-plot-act="export-png">
                <i data-lucide="image"></i> PNG
              </button>
              <button type="button" class="btn btn-secondary btn-xs" data-plot-act="export-svg">
                <i data-lucide="file-code"></i> SVG
              </button>
              <button type="button" class="btn btn-secondary btn-xs" data-plot-act="export-tsv">
                <i data-lucide="table"></i> TSV
              </button>
            </div>
          </div>
          <div class="panel-body plots-preview-body">
            <div id="plots-chart" class="plots-chart"></div>
            <p class="form-hint plots-preview-hint">${t('plots.preview_hint')}</p>
          </div>
        </div>
      </div>
    </div>`;
}

// ---------- binding ----------

function bind(container) {
  container.querySelectorAll('[data-plot-type]').forEach(btn => {
    btn.addEventListener('click', () => {
      resetWith(btn.dataset.plotType, { loadSample: true });
    });
  });

  container.addEventListener('click', (ev) => {
    const target = ev.target.closest('[data-plot-act]');
    if (!target) return;
    const act = target.dataset.plotAct;
    if (act === 'load-sample') resetWith(local.chartType, { loadSample: true });
    else if (act === 'reset') resetWith(local.chartType, { loadSample: false });
    else if (act === 'render') render();
    else if (act === 'export-png' && local.chartInstance) {
      exportPng(local.chartInstance, `rustbrain_${local.chartType}`);
    }
    else if (act === 'export-svg' && local.lastOption) {
      exportSvg(local.lastOption, `rustbrain_${local.chartType}`);
    }
    else if (act === 'export-tsv' && local.table) {
      exportTsv(local.table, `rustbrain_${local.chartType}`);
    }
  });

  const textarea = container.querySelector('#plots-data');
  textarea.addEventListener('input', () => onDataChanged(textarea.value));

  const fileInput = container.querySelector('#plots-file-input');
  fileInput.addEventListener('change', async () => {
    const f = fileInput.files && fileInput.files[0];
    if (!f) return;
    const text = await f.text();
    textarea.value = text;
    onDataChanged(text);
    fileInput.value = '';
  });

  local.resizeHandler = () => { if (local.chartInstance) local.chartInstance.resize(); };
  window.addEventListener('resize', local.resizeHandler);

  const chartEl = container.querySelector('#plots-chart');
  if ('ResizeObserver' in window && chartEl) {
    local.resizeObs = new ResizeObserver(() => {
      if (local.chartInstance) local.chartInstance.resize();
    });
    local.resizeObs.observe(chartEl);
  }
}

function resetWith(typeId, { loadSample }) {
  const chart = findChart(typeId);
  if (!chart) return;
  local.chartType = typeId;

  document.querySelectorAll('[data-plot-type]').forEach(el => {
    el.classList.toggle('active', el.dataset.plotType === typeId);
  });
  const title = document.getElementById('plots-preview-title');
  if (title) title.textContent = `${t('plots.preview')} — ${t(chart.meta.label_key)}`;

  const textarea = document.getElementById('plots-data');
  if (loadSample) textarea.value = chart.sample();
  // reset stored params for this chart so `defaults(table)` is re-applied
  delete local.params[typeId];

  onDataChanged(textarea.value);
  renderParamsPanel();
  render();
}

function onDataChanged(text) {
  try {
    local.table = parseTable(text);
  } catch (_) {
    local.table = null;
  }
  const summary = document.getElementById('plots-data-summary');
  if (summary) {
    if (local.table && local.table.header.length) {
      summary.innerHTML =
        `<span class="plot-chip">${local.table.header.length} ${t('plots.columns')}</span>` +
        `<span class="plot-chip">${local.table.rows.length} ${t('plots.rows')}</span>` +
        `<span class="plot-chip plot-chip-dim">${t('plots.columns')}: ${local.table.header.slice(0, 6).map(escapeHtml).join(', ')}${local.table.header.length > 6 ? '…' : ''}</span>`;
    } else {
      summary.innerHTML = '';
    }
  }
  renderParamsPanel();
}

// ---------- params panel ----------

function effectiveParams(chart) {
  const fallback = chart.defaults(local.table) || {};
  const user = local.params[chart.meta.id] || {};
  return { ...fallback, ...user };
}

function renderParamsPanel() {
  const panel = document.getElementById('plots-params');
  if (!panel) return;
  const chart = findChart(local.chartType);
  if (!chart) return;

  const params = effectiveParams(chart);
  const helpers = {
    t, colOptions: makeColOptionsFn(local.table),
    escapeHtml, guessIdx, header: local.table?.header || [],
  };
  panel.innerHTML = chart.renderParams(local.table, params, helpers);

  panel.querySelectorAll('[data-plot-param]').forEach(el => {
    const handler = () => captureParams();
    el.addEventListener('change', handler);
    el.addEventListener('input', handler);
  });
  if (window.lucide) window.lucide.createIcons();
}

function captureParams() {
  const panel = document.getElementById('plots-params');
  if (!panel) return;
  const p = {};
  panel.querySelectorAll('[data-plot-param]').forEach(el => {
    const key = el.dataset.plotParam;
    if (el.type === 'checkbox') p[key] = el.checked;
    else if (el.type === 'number') p[key] = el.value === '' ? null : Number(el.value);
    else p[key] = el.value;
  });
  local.params[local.chartType] = p;
}

function makeColOptionsFn(table) {
  const header = table?.header || [];
  return (selected, allowEmpty = false) => {
    const empty = allowEmpty ? `<option value="">${t('plots.none')}</option>` : '';
    return empty + header.map((h, i) =>
      `<option value="${i}" ${String(i) === String(selected) ? 'selected' : ''}>${escapeHtml(h)} (${i})</option>`
    ).join('');
  };
}

// ---------- rendering ----------

function render() {
  captureParams();
  const container = document.getElementById('plots-chart');
  const chart = findChart(local.chartType);
  if (!container || !local.table || !chart) return;
  if (!window.echarts) {
    container.innerHTML = `<p class="form-hint" style="padding:20px">ECharts not loaded.</p>`;
    return;
  }

  if (local.chartInstance) {
    try { local.chartInstance.dispose(); } catch (_) {}
  }
  local.chartInstance = window.echarts.init(container, null, { renderer: 'canvas' });

  let option;
  try {
    option = chart.build(local.table, effectiveParams(chart));
  } catch (err) {
    console.error('[plots] render failed', err);
    option = {
      graphic: {
        type: 'text', left: 'center', top: 'middle',
        style: { text: String(err && err.message || err), fill: '#c9503c' },
      },
    };
  }

  if (!option) return;
  local.lastOption = option;
  local.chartInstance.setOption(option, true);
}

// ---------- util ----------

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function guessIdx(header, candidates) {
  const lower = header.map(h => String(h || '').toLowerCase());
  for (const cand of candidates) {
    const idx = lower.indexOf(cand);
    if (idx >= 0) return idx;
  }
  return null;
}
