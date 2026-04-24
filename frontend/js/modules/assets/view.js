import { t } from '../../core/i18n-helpers.js';
import { assetsApi } from '../../api/assets.js';
import { escapeHtml } from '../../ui/escape.js';
import { confirmModal, alertModal } from '../../ui/modal.js';
import { formatBytes } from '../run-result.js';

const KIND_KEYS = ['StarIndex', 'Bam', 'TrimmedFastq', 'Gtf', 'CountsMatrix', 'Report', 'Other'];

const viewState = {
  assets: [],
  filterKind: 'all',
};

function i18nKind(kind) {
  const k = String(kind || 'Other').toLowerCase();
  return t(`assets.kind.${k}`);
}

function kindPillHtml(kind) {
  const cls = `kind-pill kind-${String(kind || 'Other').toLowerCase()}`;
  return `<span class="${cls}">${escapeHtml(i18nKind(kind))}</span>`;
}

function filtered() {
  return viewState.assets.filter(a => {
    if (viewState.filterKind !== 'all' && a.kind !== viewState.filterKind) return false;
    return true;
  });
}

function renderRow(a) {
  const created = a.created_at || '';
  return `
    <tr data-asset-id="${escapeHtml(a.id)}">
      <td>${kindPillHtml(a.kind)}</td>
      <td class="inputs-col-name"><code>${escapeHtml(a.display_name)}</code></td>
      <td class="inputs-col-path" title="${escapeHtml(a.path)}"><code>${escapeHtml(a.path)}</code></td>
      <td class="inputs-col-size">${escapeHtml(formatBytes(a.size_bytes || 0))}</td>
      <td class="inputs-col-time"><code>${escapeHtml(a.produced_by_run_id)}</code></td>
      <td class="inputs-col-time">${escapeHtml(created)}</td>
      <td class="inputs-col-actions">
        <button type="button" class="btn btn-ghost btn-sm"
                data-act="assets-delete-row"
                data-asset-id="${escapeHtml(a.id)}"
                title="${escapeHtml(t('assets.delete'))}">
          <i data-lucide="trash-2"></i>
        </button>
      </td>
    </tr>`;
}

function renderTable() {
  const tbody = document.getElementById('assets-table-body');
  if (!tbody) return;
  const rows = filtered();
  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="7" class="samples-empty">${escapeHtml(t('assets.empty'))}</td></tr>`;
  } else {
    tbody.innerHTML = rows.map(renderRow).join('');
  }
  const countEl = document.getElementById('assets-count');
  const sizeEl = document.getElementById('assets-total-size');
  if (countEl) countEl.textContent = t('assets.count_label', { n: rows.length });
  if (sizeEl) {
    const total = rows.reduce((a, r) => a + (r.size_bytes || 0), 0);
    sizeEl.textContent = t('assets.total_size_label', { size: formatBytes(total) });
  }
  if (window.lucide) window.lucide.createIcons();
}

function renderKindFilter() {
  const sel = document.getElementById('assets-filter-kind');
  if (!sel) return;
  const opts = [`<option value="all">${escapeHtml(t('assets.all_kinds'))}</option>`];
  KIND_KEYS.forEach(k => {
    const selected = viewState.filterKind === k ? 'selected' : '';
    opts.push(`<option value="${k}" ${selected}>${escapeHtml(i18nKind(k))}</option>`);
  });
  sel.innerHTML = opts.join('');
}

async function loadAll() {
  try {
    const rows = await assetsApi.list();
    viewState.assets = (rows || []).sort((a, b) =>
      String(b.created_at || '').localeCompare(String(a.created_at || ''))
    );
  } catch (err) {
    viewState.assets = [];
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
  renderKindFilter();
  renderTable();
}

async function handleDelete(id) {
  const ok = await confirmModal({
    title: t('assets.delete_confirm_title'),
    message: t('assets.delete_confirm_message'),
    okLabel: t('assets.delete'),
  });
  if (!ok) return;
  try {
    await assetsApi.delete(id);
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

function bindEvents(container) {
  container.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    if (btn.dataset.act === 'assets-delete-row') handleDelete(btn.dataset.assetId);
    else if (btn.dataset.act === 'assets-refresh') loadAll();
  });
  container.addEventListener('change', (e) => {
    if (e.target.id === 'assets-filter-kind') {
      viewState.filterKind = e.target.value;
      renderTable();
    }
  });
}

export function renderAssetsView(container) {
  container.innerHTML = `
    <div class="module-view assets-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(124,92,191,0.12); color: #7c5cbf;">
          <i data-lucide="package"></i>
        </div>
        <div>
          <h1 class="module-title">${escapeHtml(t('assets.title'))}</h1>
          <p class="module-desc">${escapeHtml(t('assets.subtitle'))}</p>
        </div>
      </div>

      <div class="card inputs-toolbar">
        <div class="inputs-toolbar-row">
          <label class="inputs-filter">
            <span>${escapeHtml(t('assets.filter_kind'))}</span>
            <select id="assets-filter-kind"></select>
          </label>
          <button type="button" class="btn btn-secondary" data-act="assets-refresh">
            <i data-lucide="refresh-cw"></i> ${escapeHtml(t('common.refresh'))}
          </button>
        </div>
        <div class="inputs-summary">
          <span id="assets-count"></span>
          <span id="assets-total-size"></span>
        </div>
      </div>

      <div class="card inputs-table-card">
        <table class="inputs-table">
          <thead>
            <tr>
              <th>${escapeHtml(t('assets.col_kind'))}</th>
              <th>${escapeHtml(t('assets.col_name'))}</th>
              <th>${escapeHtml(t('assets.col_path'))}</th>
              <th>${escapeHtml(t('assets.col_size'))}</th>
              <th>${escapeHtml(t('assets.col_source'))}</th>
              <th>${escapeHtml(t('assets.col_time'))}</th>
              <th>${escapeHtml(t('assets.col_actions'))}</th>
            </tr>
          </thead>
          <tbody id="assets-table-body">
            <tr><td colspan="7">${escapeHtml(t('common.loading'))}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  `;
  bindEvents(container);
  loadAll();
  if (window.lucide) window.lucide.createIcons();
}
