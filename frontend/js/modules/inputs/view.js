import { t } from '../../core/i18n-helpers.js';
import { inputsApi } from '../../api/inputs.js';
import { filesApi } from '../../api/files.js';
import { escapeHtml } from '../../ui/escape.js';
import { confirmModal, alertModal, showToast } from '../../ui/modal.js';
import { formatBytes } from '../run-result.js';

const KIND_KEYS = ['Fastq', 'Fasta', 'Gtf', 'Gff', 'CountsMatrix', 'SampleSheet', 'Other'];

const viewState = {
  inputs: [],
  filterKind: 'all',
  filterMissing: false,
  search: '',
  selected: new Set(),
};

function i18nKind(kind) {
  const k = String(kind || 'Other').toLowerCase();
  return t(`inputs.kind.${k}`);
}

function kindPillHtml(kind) {
  const cls = `kind-pill kind-${String(kind || 'Other').toLowerCase()}`;
  return `<span class="${cls}">${escapeHtml(i18nKind(kind))}</span>`;
}

function filteredInputs() {
  const q = viewState.search.trim().toLowerCase();
  return viewState.inputs.filter(r => {
    if (viewState.filterKind !== 'all' && r.kind !== viewState.filterKind) return false;
    if (viewState.filterMissing && !r.missing) return false;
    if (q) {
      const hay = `${r.display_name}\n${r.path}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
}

function renderRow(rec) {
  const checked = viewState.selected.has(rec.id) ? 'checked' : '';
  const rowCls = rec.missing ? 'input-missing' : '';
  const regAt = rec.registered_at || '';
  return `
    <tr class="${rowCls}" data-input-id="${escapeHtml(rec.id)}">
      <td class="inputs-col-check">
        <input type="checkbox" class="inputs-row-check" data-input-id="${escapeHtml(rec.id)}" ${checked}/>
      </td>
      <td>${kindPillHtml(rec.kind)}</td>
      <td class="inputs-col-name">
        <input type="text" class="inputs-name-edit"
               data-input-id="${escapeHtml(rec.id)}"
               value="${escapeHtml(rec.display_name)}"
               aria-label="${escapeHtml(t('inputs.col_name'))}"/>
      </td>
      <td class="inputs-col-path" title="${escapeHtml(rec.path)}"><code>${escapeHtml(rec.path)}</code></td>
      <td class="inputs-col-size">${escapeHtml(formatBytes(rec.size_bytes || 0))}</td>
      <td class="inputs-col-time">${escapeHtml(regAt)}</td>
      <td class="inputs-col-status">${rec.missing ? `<span class="status-pill status-missing">${escapeHtml(t('inputs.missing_badge'))}</span>` : ''}</td>
      <td class="inputs-col-actions">
        <button type="button" class="btn btn-ghost btn-sm"
                data-act="inputs-delete-row"
                data-input-id="${escapeHtml(rec.id)}"
                title="${escapeHtml(t('inputs.delete'))}">
          <i data-lucide="trash-2"></i>
        </button>
      </td>
    </tr>`;
}

function renderTable() {
  const tbody = document.getElementById('inputs-table-body');
  if (!tbody) return;
  const rows = filteredInputs();
  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="8" class="inputs-empty">${escapeHtml(t('inputs.empty'))}</td></tr>`;
  } else {
    tbody.innerHTML = rows.map(renderRow).join('');
  }
  const countEl = document.getElementById('inputs-count');
  const sizeEl = document.getElementById('inputs-total-size');
  if (countEl) countEl.textContent = t('inputs.count_label', { n: rows.length });
  if (sizeEl) {
    const total = rows.reduce((a, r) => a + (r.size_bytes || 0), 0);
    sizeEl.textContent = t('inputs.total_size_label', { size: formatBytes(total) });
  }
  if (window.lucide) window.lucide.createIcons();
}

function renderKindFilter() {
  const sel = document.getElementById('inputs-filter-kind');
  if (!sel) return;
  const opts = [`<option value="all">${escapeHtml(t('inputs.all_kinds'))}</option>`];
  KIND_KEYS.forEach(k => {
    const selected = viewState.filterKind === k ? 'selected' : '';
    opts.push(`<option value="${k}" ${selected}>${escapeHtml(i18nKind(k))}</option>`);
  });
  sel.innerHTML = opts.join('');
}

async function loadAll() {
  try {
    const rows = await inputsApi.list();
    viewState.inputs = (rows || []).sort((a, b) => {
      return String(b.registered_at || '').localeCompare(String(a.registered_at || ''));
    });
    const alive = new Set(viewState.inputs.map(r => r.id));
    [...viewState.selected].forEach(id => { if (!alive.has(id)) viewState.selected.delete(id); });
  } catch (err) {
    viewState.inputs = [];
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
  renderKindFilter();
  renderTable();
}

async function handleRegisterFiles() {
  let picked;
  try {
    picked = await filesApi.selectFiles({ multiple: true });
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
    return;
  }
  if (!picked || picked.length === 0) return;

  try {
    const result = await inputsApi.registerBatch(picked);
    const added = result?.registered?.length ?? 0;
    const errs = result?.errors ?? [];
    showToast({
      title: t('inputs.register_toast_title'),
      message: t('inputs.register_toast_message', { n: added }),
    });
    if (errs.length) {
      alertModal({
        title: t('status.error_prefix'),
        message: errs.map(e => `${e.path}: ${e.message}`).join('\n'),
      });
    }
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleScan() {
  try {
    const r = await inputsApi.scan();
    showToast({
      title: t('inputs.scan_toast_title'),
      message: t('inputs.scan_toast_message', {
        refreshed: r?.refreshed ?? 0,
        now_missing: r?.now_missing ?? 0,
        recovered: r?.recovered ?? 0,
      }),
    });
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleDeleteOne(id) {
  const ok = await confirmModal({
    title: t('inputs.delete_confirm_title'),
    message: t('inputs.delete_confirm_message', { n: 1 }),
    okLabel: t('inputs.delete'),
  });
  if (!ok) return;
  try {
    await inputsApi.delete(id);
    viewState.selected.delete(id);
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleDeleteSelected() {
  if (viewState.selected.size === 0) return;
  const n = viewState.selected.size;
  const ok = await confirmModal({
    title: t('inputs.delete_confirm_title'),
    message: t('inputs.delete_confirm_message', { n }),
    okLabel: t('inputs.delete'),
  });
  if (!ok) return;
  const ids = [...viewState.selected];
  const errors = [];
  for (const id of ids) {
    try { await inputsApi.delete(id); }
    catch (err) { errors.push(`${id}: ${err}`); }
  }
  viewState.selected.clear();
  if (errors.length) {
    alertModal({ title: t('status.error_prefix'), message: errors.join('\n') });
  }
  await loadAll();
}

async function handleRename(id, newName) {
  const trimmed = (newName || '').trim();
  if (!trimmed) return;
  try {
    await inputsApi.update(id, { display_name: trimmed });
    const rec = viewState.inputs.find(r => r.id === id);
    if (rec) rec.display_name = trimmed;
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
    await loadAll();
  }
}

function bindEvents(container) {
  container.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    switch (btn.dataset.act) {
      case 'inputs-register-files': handleRegisterFiles(); break;
      case 'inputs-scan':            handleScan(); break;
      case 'inputs-delete-selected': handleDeleteSelected(); break;
      case 'inputs-delete-row':      handleDeleteOne(btn.dataset.inputId); break;
    }
  });

  container.addEventListener('change', (e) => {
    if (e.target.id === 'inputs-filter-kind') {
      viewState.filterKind = e.target.value;
      renderTable();
    } else if (e.target.id === 'inputs-filter-missing') {
      viewState.filterMissing = e.target.checked;
      renderTable();
    } else if (e.target.id === 'inputs-select-all') {
      const rows = filteredInputs();
      if (e.target.checked) rows.forEach(r => viewState.selected.add(r.id));
      else rows.forEach(r => viewState.selected.delete(r.id));
      renderTable();
    } else if (e.target.classList?.contains('inputs-row-check')) {
      const id = e.target.dataset.inputId;
      if (e.target.checked) viewState.selected.add(id);
      else viewState.selected.delete(id);
    }
  });

  container.addEventListener('input', (e) => {
    if (e.target.id === 'inputs-search') {
      viewState.search = e.target.value;
      renderTable();
    }
  });

  container.addEventListener('blur', (e) => {
    if (e.target.classList?.contains('inputs-name-edit')) {
      handleRename(e.target.dataset.inputId, e.target.value);
    }
  }, true);

  container.addEventListener('keydown', (e) => {
    if (e.target.classList?.contains('inputs-name-edit') && e.key === 'Enter') {
      e.preventDefault();
      e.target.blur();
    }
  });
}

export function renderInputsView(container) {
  container.innerHTML = `
    <div class="module-view inputs-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(13,115,119,0.12); color: #0d7377;">
          <i data-lucide="database"></i>
        </div>
        <div>
          <h1 class="module-title">${escapeHtml(t('inputs.title'))}</h1>
          <p class="module-desc">${escapeHtml(t('inputs.subtitle'))}</p>
        </div>
      </div>

      <div class="card inputs-toolbar">
        <div class="inputs-toolbar-row">
          <button type="button" class="btn btn-primary" data-act="inputs-register-files">
            <i data-lucide="file-plus"></i> ${escapeHtml(t('inputs.register_files'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="inputs-scan">
            <i data-lucide="refresh-cw"></i> ${escapeHtml(t('inputs.scan'))}
          </button>
          <label class="inputs-filter">
            <span>${escapeHtml(t('inputs.filter_kind'))}</span>
            <select id="inputs-filter-kind"></select>
          </label>
          <label class="inputs-filter">
            <input type="checkbox" id="inputs-filter-missing"/>
            <span>${escapeHtml(t('inputs.filter_missing'))}</span>
          </label>
          <input type="search" id="inputs-search" class="inputs-search"
                 placeholder="${escapeHtml(t('inputs.search_placeholder'))}"/>
          <div class="inputs-toolbar-spacer"></div>
          <button type="button" class="btn btn-danger" data-act="inputs-delete-selected">
            <i data-lucide="trash-2"></i> ${escapeHtml(t('inputs.delete_selected'))}
          </button>
        </div>
        <div class="inputs-summary">
          <span id="inputs-count"></span>
          <span id="inputs-total-size"></span>
        </div>
      </div>

      <div class="card inputs-table-card">
        <table class="inputs-table">
          <thead>
            <tr>
              <th class="inputs-col-check">
                <input type="checkbox" id="inputs-select-all" aria-label="${escapeHtml(t('inputs.select_all'))}"/>
              </th>
              <th>${escapeHtml(t('inputs.col_kind'))}</th>
              <th>${escapeHtml(t('inputs.col_name'))}</th>
              <th>${escapeHtml(t('inputs.col_path'))}</th>
              <th>${escapeHtml(t('inputs.col_size'))}</th>
              <th>${escapeHtml(t('inputs.col_registered'))}</th>
              <th>${escapeHtml(t('inputs.col_status'))}</th>
              <th>${escapeHtml(t('inputs.col_actions'))}</th>
            </tr>
          </thead>
          <tbody id="inputs-table-body">
            <tr><td colspan="8">${escapeHtml(t('common.loading'))}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  `;

  bindEvents(container);
  loadAll();
  if (window.lucide) window.lucide.createIcons();
}
