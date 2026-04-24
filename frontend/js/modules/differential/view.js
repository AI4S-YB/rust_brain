import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { attachAssetPicker, attachInputPicker, invalidatePickerCache } from '../../ui/registry-picker.js';
import { renderFileList } from '../../ui/file-drop.js';
import { filesApi } from '../../api/files.js';
import { inputsApi } from '../../api/inputs.js';
import { escapeHtml } from '../../ui/escape.js';

let countsPreviewRequest = 0;

function getSheet() {
  if (!state.sampleSheet) {
    state.sampleSheet = {
      columns: ['sample', 'condition'],
      rows: [],
      filename: '',
      source: '',
      sourcePath: '',
      status: '',
    };
  }
  return state.sampleSheet;
}

export function renderDifferentialView(container) {
  const mod = { id: 'differential', icon: 'flame', color: 'coral', tool: 'DESeq2_rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderDifferentialBody()}</div>`;

  const host = container.querySelector('.registry-picker[data-kind="asset"][data-asset-kind="CountsMatrix"]');
  const inputHost = container.querySelector('.registry-picker[data-kind="input"][data-input-kind="CountsMatrix"]');
  const coldataHost = container.querySelector('.registry-picker[data-kind="input"][data-input-kind="SampleSheet"]');

  const setCountsFile = (rec, lineageKey) => {
    if (!rec) {
      state.files.differential = [];
    } else {
      state.files.differential = [{
        name: rec.display_name,
        path: rec.path,
        size: rec.size_bytes || 0,
        inputId: lineageKey === 'input' ? rec.id : undefined,
        assetId: lineageKey === 'asset' ? rec.id : undefined,
      }];
    }
    const list = document.getElementById('differential-file-list');
    if (list) renderFileList(list, 'differential');
    renderCountsMatrixPreview(rec?.path || '');
    syncSampleSheetFromMatrix(rec?.path || '');
  };

  const coldataOnPick = (input) => {
    state.files['differential-coldata'] = input
      ? [{ name: input.display_name, path: input.path, size: input.size_bytes || 0, inputId: input.id }]
      : [];
    if (coldataHost) coldataHost.dataset.selectedInputIds = input?.id || '';
    const list = document.getElementById('differential-coldata-file-list');
    if (list) renderFileList(list, 'differential-coldata');
    if (input?.path) loadSampleSheetFromColdata(input.path);
  };

  container.addEventListener('files-changed', (event) => {
    const m = event.detail?.module;
    if (m === 'differential') {
      renderCountsMatrixPreview();
      syncSampleSheetFromMatrix();
    } else if (m === 'differential-coldata') {
      const p = state.files['differential-coldata']?.[0]?.path;
      if (p) loadSampleSheetFromColdata(p);
    }
  });

  if (host) {
    attachAssetPicker(host, (asset) => {
      if (asset && inputHost) {
        inputHost.dataset.selectedId = '';
        inputHost.dataset.selectedInputIds = '';
        const inputSelect = inputHost.querySelector('select');
        if (inputSelect) inputSelect.value = '';
      }
      setCountsFile(asset, 'asset');
    });
  }
  if (inputHost) {
    attachInputPicker(inputHost, (input) => {
      if (input && host) {
        host.dataset.selectedId = '';
        const assetSelect = host.querySelector('select');
        if (assetSelect) assetSelect.value = '';
      }
      setCountsFile(input, 'input');
      inputHost.dataset.selectedInputIds = input?.id || '';
    });
  }
  if (coldataHost) attachInputPicker(coldataHost, coldataOnPick);

  const countsInput = container.querySelector('#deseq-counts-matrix');
  if (countsInput) {
    countsInput.addEventListener('input', () => {
      renderCountsMatrixPreview(countsInput.value);
      syncSampleSheetFromMatrix(countsInput.value);
    });
  }

  // --- Sample-sheet editor listeners (scoped to this container) ---
  container.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-sheet-act]');
    if (!btn) return;
    const act = btn.dataset.sheetAct;
    if (act === 'add-col') {
      addSheetColumn();
    } else if (act === 'del-col') {
      const idx = Number(btn.dataset.col);
      if (Number.isInteger(idx)) deleteSheetColumn(idx);
    } else if (act === 'save') {
      saveSampleSheet(btn, coldataHost, coldataOnPick);
    }
  });

  container.addEventListener('input', (e) => {
    const cell = e.target.closest('[data-sheet-cell]');
    if (cell) {
      const r = Number(cell.dataset.row);
      const c = Number(cell.dataset.col);
      const sheet = getSheet();
      if (sheet.rows[r]) sheet.rows[r][c] = e.target.value;
      return;
    }
    const colHead = e.target.closest('[data-sheet-col-header]');
    if (colHead) {
      const c = Number(colHead.dataset.col);
      const sheet = getSheet();
      if (sheet.columns[c] !== undefined && c > 0) sheet.columns[c] = e.target.value;
      return;
    }
    if (e.target.id === 'differential-sheet-filename') {
      getSheet().filename = e.target.value.trim();
    }
  });

  renderCountsMatrixPreview(countsInput?.value || '');
  renderSampleSheetEditor();

  // If a counts matrix path is already set (e.g. prefill or persisted state), seed the sheet.
  const initialMatrix = countsInput?.value || state.files.differential?.[0]?.path || '';
  if (initialMatrix) syncSampleSheetFromMatrix(initialMatrix);

  if (window.lucide) window.lucide.createIcons();
}

function currentCountsMatrixPath(explicitPath = '') {
  if (explicitPath) return explicitPath;
  const input = document.getElementById('deseq-counts-matrix');
  if (input?.value) return input.value;
  return state.files.differential?.[0]?.path || '';
}

function renderCountsMatrixPreview(explicitPath = '') {
  const el = document.getElementById('differential-counts-preview');
  if (!el) return;
  const path = currentCountsMatrixPath(explicitPath);
  const requestId = ++countsPreviewRequest;
  if (!path) {
    el.innerHTML = `<p><em>${escapeHtml(t('differential.preview_empty'))}</em></p>`;
    return;
  }
  el.innerHTML = `<p><em>${escapeHtml(t('common.loading'))}</em></p>`;
  filesApi.readTablePreview(path, { maxRows: 50, maxCols: 10, hasHeader: true })
    .then(preview => {
      if (requestId !== countsPreviewRequest) return;
      el.innerHTML = renderTablePreview(preview);
    })
    .catch(err => {
      if (requestId !== countsPreviewRequest) return;
      el.innerHTML = `<p><em>${escapeHtml(t('differential.preview_failed'))}: ${escapeHtml(String(err))}</em></p>`;
    });
}

function renderTablePreview(preview) {
  const headers = Array.isArray(preview?.headers) ? preview.headers : [];
  const rows = Array.isArray(preview?.rows) ? preview.rows : [];
  if (headers.length === 0 && rows.length === 0) {
    return `<p><em>${escapeHtml(t('differential.preview_empty'))}</em></p>`;
  }
  const head = headers.length
    ? `<thead><tr>${headers.map(c => `<th>${escapeHtml(c)}</th>`).join('')}</tr></thead>`
    : '';
  const body = rows
    .map(r => '<tr>' + r.map(c => `<td>${escapeHtml(c)}</td>`).join('') + '</tr>')
    .join('');
  return `<table class="data-table">${head}<tbody>${body}</tbody></table>`;
}

// --- Sample-sheet editor ---

async function syncSampleSheetFromMatrix(explicitPath = '') {
  const path = currentCountsMatrixPath(explicitPath);
  if (!path) return;
  try {
    const preview = await filesApi.readTablePreview(path, { maxRows: 1, maxCols: 10000, hasHeader: true });
    const headers = (preview?.headers || []).map(String);
    if (headers.length < 2) return;
    const newSamples = headers.slice(1);
    const sheet = getSheet();
    const prevBySample = new Map(sheet.rows.map(r => [r[0], r]));
    sheet.rows = newSamples.map(name => {
      const prev = prevBySample.get(name);
      if (prev) {
        // Keep prev values but align length to current columns.
        const aligned = sheet.columns.map((_, i) => prev[i] ?? '');
        aligned[0] = name;
        return aligned;
      }
      return sheet.columns.map((_, i) => (i === 0 ? name : ''));
    });
    sheet.status = t('differential.sheet_from_matrix').replace('{n}', String(newSamples.length));
    renderSampleSheetEditor();
  } catch (err) {
    console.warn('syncSampleSheetFromMatrix', err);
  }
}

async function loadSampleSheetFromColdata(coldataPath) {
  if (!coldataPath) return;
  try {
    const preview = await filesApi.readTablePreview(coldataPath, { maxRows: 10000, maxCols: 100, hasHeader: true });
    let headers = (preview?.headers || []).map(String);
    const rows = (preview?.rows || []).map(r => r.map(String));
    if (headers.length === 0 || rows.length === 0) return;
    if (!headers[0] || !headers[0].trim()) headers[0] = 'sample';
    const sheet = getSheet();
    sheet.columns = headers;
    sheet.rows = rows.map(row => headers.map((_, i) => row[i] ?? ''));
    sheet.source = 'coldata';
    sheet.sourcePath = coldataPath;
    sheet.status = t('differential.sheet_from_coldata');
    renderSampleSheetEditor();
  } catch (err) {
    console.warn('loadSampleSheetFromColdata', err);
  }
}

function addSheetColumn() {
  const sheet = getSheet();
  let base = t('differential.sheet_col_new') || 'new_col';
  let name = base;
  let n = 1;
  while (sheet.columns.includes(name)) {
    n += 1;
    name = `${base}_${n}`;
  }
  sheet.columns.push(name);
  sheet.rows.forEach(r => r.push(''));
  renderSampleSheetEditor();
}

function deleteSheetColumn(colIdx) {
  if (colIdx <= 0) return; // Never remove the sample column.
  const sheet = getSheet();
  if (colIdx >= sheet.columns.length) return;
  if (!confirm(t('differential.sheet_confirm_clear_col'))) return;
  sheet.columns.splice(colIdx, 1);
  sheet.rows.forEach(r => r.splice(colIdx, 1));
  renderSampleSheetEditor();
}

async function saveSampleSheet(btn, coldataHost, coldataOnPick) {
  const sheet = getSheet();
  if (!sheet.rows.length) return;
  const headers = [...sheet.columns];
  const rows = sheet.rows.map(r => headers.map((_, i) => String(r[i] ?? '')));
  btn.disabled = true;
  try {
    const rec = await inputsApi.writeSampleSheet({
      filename: sheet.filename,
      headers,
      rows,
    });
    sheet.sourcePath = rec.path;
    sheet.source = 'coldata';
    sheet.status = t('differential.sheet_saved').replace('{name}', rec.display_name);
    // Mirror the selection into the coldata picker + file-list so downstream
    // (run_module coldata_path) picks up the newly-written sheet without the
    // user having to click the picker again.
    invalidatePickerCache();
    if (coldataHost) {
      await attachInputPicker(coldataHost, coldataOnPick);
      const select = coldataHost.querySelector('select');
      if (select) {
        select.value = rec.id;
        select.dispatchEvent(new Event('change', { bubbles: true }));
      }
    }
  } catch (err) {
    sheet.status = `${t('differential.sheet_save_failed')}: ${err}`;
  } finally {
    btn.disabled = false;
    renderSampleSheetEditor();
  }
}

function renderSampleSheetEditor() {
  const host = document.getElementById('differential-sample-sheet-editor');
  if (!host) return;
  const sheet = getSheet();

  const hasRows = sheet.rows.length > 0;
  const statusHtml = sheet.status
    ? `<p class="form-hint" style="margin:0 0 8px 0">${escapeHtml(sheet.status)}</p>`
    : `<p class="form-hint" style="margin:0 0 8px 0">${escapeHtml(t('differential.sheet_empty'))}</p>`;

  let tableHtml = '';
  if (hasRows) {
    const headRow = sheet.columns.map((c, i) => {
      if (i === 0) {
        return `<th style="background:var(--surface-alt);padding:6px 10px;text-align:left;font-weight:600">${escapeHtml(c)}</th>`;
      }
      const delBtn = `<button type="button" class="btn btn-ghost" data-sheet-act="del-col" data-col="${i}" title="${escapeHtml(t('differential.sheet_del_col'))}" style="padding:2px 6px;margin-left:4px;font-size:0.85rem">×</button>`;
      return `<th style="background:var(--surface-alt);padding:4px 6px;text-align:left">
        <input type="text" class="form-input" data-sheet-col-header data-col="${i}" value="${escapeHtml(c)}" placeholder="${escapeHtml(t('differential.sheet_col_rename_ph'))}" style="width:calc(100% - 28px);padding:3px 6px;font-size:0.82rem;font-weight:600">
        ${delBtn}
      </th>`;
    }).join('');
    const bodyRows = sheet.rows.map((row, r) => {
      const cells = sheet.columns.map((_, c) => {
        const val = row[c] ?? '';
        if (c === 0) {
          return `<td style="padding:4px 10px;color:var(--text-strong);font-family:var(--font-mono, monospace);font-size:0.85rem">${escapeHtml(String(val))}</td>`;
        }
        return `<td style="padding:3px 6px">
          <input type="text" class="form-input" data-sheet-cell data-row="${r}" data-col="${c}" value="${escapeHtml(String(val))}" style="padding:3px 6px;font-size:0.85rem;min-width:100px">
        </td>`;
      }).join('');
      return `<tr>${cells}</tr>`;
    }).join('');
    tableHtml = `<div style="overflow-x:auto;border:1px solid var(--border);border-radius:6px">
      <table class="data-table" style="width:100%;border-collapse:collapse">
        <thead><tr>${headRow}</tr></thead>
        <tbody>${bodyRows}</tbody>
      </table>
    </div>`;
  }

  const controls = `
    <div class="form-row" style="margin-top:10px;align-items:flex-end;gap:8px">
      <div class="form-group" style="flex:1;margin:0">
        <label class="form-label">${escapeHtml(t('differential.sheet_filename_label'))}</label>
        <input type="text" id="differential-sheet-filename" class="form-input" value="${escapeHtml(sheet.filename)}" placeholder="${escapeHtml(t('differential.sheet_filename_ph'))}">
      </div>
      <button type="button" class="btn btn-secondary btn-sm" data-sheet-act="add-col" ${hasRows ? '' : 'disabled'}>
        ${escapeHtml(t('differential.sheet_add_col'))}
      </button>
      <button type="button" class="btn btn-primary btn-sm" data-sheet-act="save" ${hasRows ? '' : 'disabled'}
              style="background:var(--mod-coral);border-color:var(--mod-coral)">
        <i data-lucide="save"></i> ${escapeHtml(t('differential.sheet_save'))}
      </button>
    </div>`;

  host.innerHTML = statusHtml + tableHtml + controls;
  if (window.lucide) window.lucide.createIcons();
}

function renderDifferentialBody() {
  const prefill = (state.prefill && state.prefill.differential) || {};
  state.prefill = {};
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('differential.input_data')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('differential.counts_matrix')}</label>
              <div class="registry-picker"
                   data-kind="asset"
                   data-asset-kind="CountsMatrix"
                   data-lineage-key="asset"></div>
              <div class="registry-picker"
                   data-kind="input"
                   data-input-kind="CountsMatrix"
                   data-lineage-key="input"
                   style="margin-top:8px"></div>
              ${prefill.counts_matrix
                ? `<input type="text" class="form-input" id="deseq-counts-matrix" data-param="counts_path" value="${prefill.counts_matrix}" placeholder="${t('differential.counts_matrix_ph')}" style="margin-top:8px">`
                : `<div class="file-drop-zone" data-module="differential" data-param="counts_path" data-param-single data-accept=".tsv,.csv,.txt" style="padding:20px;margin-top:8px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="table"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_counts')}</div>
                <div class="file-drop-hint">${t('differential.drop_counts_hint')}</div>
                <div id="differential-file-list" class="file-list" style="margin-top:10px"></div>
              </div>`}
              <div class="counts-preview" style="margin-top:12px">
                <h3 style="margin:0 0 8px">${t('differential.matrix_preview')}</h3>
                <div id="differential-counts-preview"></div>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.sample_info')}</label>
              <div class="registry-picker"
                   data-kind="input"
                   data-input-kind="SampleSheet"
                   data-lineage-key="input"
                   style="margin-bottom:8px"></div>
              <div class="file-drop-zone" data-module="differential-coldata" data-param="coldata_path" data-param-single data-accept=".tsv,.csv,.txt" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_coldata')}</div>
                <div class="file-drop-hint">${t('differential.drop_coldata_hint')}</div>
                <div id="differential-coldata-file-list" class="file-list" style="margin-top:10px"></div>
              </div>
            </div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:140ms">
          <div class="panel-header"><span class="panel-title">${t('differential.sheet_title')}</span></div>
          <div class="panel-body">
            <div id="differential-sample-sheet-editor"></div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:180ms">
          <div class="panel-header"><span class="panel-title">${t('differential.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('differential.design_var')}</label>
              <input type="text" class="form-input" id="deseq-design" data-param="design" value="condition" placeholder="${t('differential.design_var_ph')}">
              <span class="form-hint">${t('differential.design_var_hint')}</span>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.ref_level')}</label>
              <input type="text" class="form-input" id="deseq-ref" data-param="reference" value="control" placeholder="${t('differential.ref_level_ph')}">
              <span class="form-hint">${t('differential.ref_level_hint')}</span>
            </div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('differential.padj')}</label><input type="number" class="form-input" id="deseq-padj" value="0.01" step="0.01" min="0" max="1"></div>
              <div class="form-group"><label class="form-label">${t('differential.lfc')}</label><input type="number" class="form-input" id="deseq-lfc" value="1.0" step="0.1" min="0"></div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.output_file')}</label>
              <input type="text" class="form-input" id="deseq-output" value="deseq2_results.tsv" placeholder="results.tsv">
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="differential"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="differential" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="differential.run_deseq" data-run-icon="play" style="background:var(--mod-coral);border-color:var(--mod-coral)"><i data-lucide="play"></i> ${t('differential.run_deseq')}</button>
          </div>
          ${renderLogPanel('differential')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="differential-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
