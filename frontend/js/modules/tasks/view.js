import { t, navKey } from '../../core/i18n-helpers.js';
import { modulesApi } from '../../api/modules.js';
import { MODULES } from '../../core/constants.js';
import { escapeHtml } from '../../ui/escape.js';
import { confirmModal, alertModal, showToast } from '../../ui/modal.js';
import { formatBytes } from '../run-result.js';
import { assetsApi } from '../../api/assets.js';

const STATUS_OPTIONS = ['Done', 'Failed', 'Cancelled', 'Running', 'Pending'];

const viewState = {
  runs: [],
  sizes: {},
  filterStatus: 'all',
  filterModule: 'all',
  selected: new Set(),
  listener: null,
};

function backendToView(backendId) {
  const m = MODULES.find(m => m.backend === backendId || m.id === backendId);
  return m || null;
}

function moduleLabel(backendId) {
  const m = backendToView(backendId);
  if (!m) return backendId;
  try { return t(navKey(m.id)); } catch { return m.name || backendId; }
}

function filteredRuns() {
  return viewState.runs.filter(r => {
    if (viewState.filterStatus !== 'all' && r.status !== viewState.filterStatus) return false;
    if (viewState.filterModule !== 'all' && r.module_id !== viewState.filterModule) return false;
    return true;
  });
}

function totalSize(runs) {
  return runs.reduce((acc, r) => acc + (viewState.sizes[r.id] || 0), 0);
}

function lineageCellHtml(run) {
  const usesIn = run.inputs_used.length;
  const usesAs = run.assets_used.length;
  const produces = run.assets_produced.length;
  if (!usesIn && !usesAs && !produces) return '<span class="tasks-lineage-empty">—</span>';
  const chips = [];
  if (usesIn) chips.push(`<span class="tasks-lineage-chip tasks-lineage-in" title="${escapeHtml(t('tasks.uses_inputs'))}">↓ ${usesIn}</span>`);
  if (usesAs) chips.push(`<span class="tasks-lineage-chip tasks-lineage-in" title="${escapeHtml(t('tasks.uses_assets'))}">↓ ${usesAs}★</span>`);
  if (produces) chips.push(`<span class="tasks-lineage-chip tasks-lineage-out" title="${escapeHtml(t('tasks.produces_assets'))}">↑ ${produces}★</span>`);
  return chips.join(' ');
}

function renderRow(run) {
  const size = viewState.sizes[run.id] || 0;
  const checked = viewState.selected.has(run.id) ? 'checked' : '';
  const blocked = run.status === 'Running' || run.status === 'Pending';
  const ts = run.finished_at || run.started_at || '';
  const mod = moduleLabel(run.module_id);
  const statusClass = `status-pill status-${run.status?.toLowerCase() || 'unknown'}`;
  return `
    <tr data-run-id="${escapeHtml(run.id)}">
      <td class="tasks-col-check">
        <input type="checkbox" class="tasks-row-check"
               data-run-id="${escapeHtml(run.id)}"
               ${checked} ${blocked ? 'disabled' : ''}/>
      </td>
      <td class="tasks-col-id"><code>${escapeHtml(run.id)}</code></td>
      <td>${escapeHtml(mod)}</td>
      <td><span class="${statusClass}">${escapeHtml(run.status || '')}</span></td>
      <td class="tasks-col-size">${formatBytes(size)}</td>
      <td class="tasks-col-lineage">${lineageCellHtml(run)}</td>
      <td class="tasks-col-time">${escapeHtml(ts)}</td>
      <td class="tasks-col-actions">
        <button type="button" class="btn btn-ghost btn-sm"
                data-act="delete-run-in-tasks"
                data-run-id="${escapeHtml(run.id)}"
                ${blocked ? 'disabled' : ''}
                title="${escapeHtml(blocked ? t('tasks.delete_blocked_running') : t('tasks.delete_run'))}">
          <i data-lucide="trash-2"></i>
        </button>
      </td>
    </tr>`;
}

function renderTable() {
  const tbody = document.getElementById('tasks-table-body');
  if (!tbody) return;
  const runs = filteredRuns();
  if (runs.length === 0) {
    tbody.innerHTML = `<tr><td colspan="8" class="tasks-empty">${t('tasks.empty')}</td></tr>`;
  } else {
    tbody.innerHTML = runs.map(renderRow).join('');
  }
  const countEl = document.getElementById('tasks-count');
  const sizeEl = document.getElementById('tasks-total-size');
  if (countEl) countEl.textContent = t('tasks.count_label', { n: runs.length });
  if (sizeEl) sizeEl.textContent = t('tasks.total_size_label', { size: formatBytes(totalSize(runs)) });
  const selEl = document.getElementById('tasks-selected-count');
  if (selEl) selEl.textContent = t('tasks.selected_label', { n: viewState.selected.size });
  if (window.lucide) window.lucide.createIcons();
}

function renderModuleFilter() {
  const select = document.getElementById('tasks-filter-module');
  if (!select) return;
  const seen = new Set(viewState.runs.map(r => r.module_id));
  const opts = [`<option value="all">${escapeHtml(t('tasks.all_modules'))}</option>`];
  [...seen].sort().forEach(mid => {
    const selected = viewState.filterModule === mid ? 'selected' : '';
    opts.push(`<option value="${escapeHtml(mid)}" ${selected}>${escapeHtml(moduleLabel(mid))}</option>`);
  });
  select.innerHTML = opts.join('');
}

async function loadAll() {
  try {
    const runs = await modulesApi.listRuns(null);
    viewState.runs = (runs || []).sort((a, b) => {
      const ak = a.finished_at || a.started_at || '';
      const bk = b.finished_at || b.started_at || '';
      return String(bk).localeCompare(String(ak));
    });
    try {
      viewState.sizes = await modulesApi.getRunSizes(viewState.runs.map(r => r.id)) || {};
    } catch {
      viewState.sizes = {};
    }
    // Drop selected ids that no longer exist.
    const alive = new Set(viewState.runs.map(r => r.id));
    [...viewState.selected].forEach(id => { if (!alive.has(id)) viewState.selected.delete(id); });
  } catch (err) {
    viewState.runs = [];
    viewState.sizes = {};
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
  renderModuleFilter();
  renderTable();
}

function bindEvents(container) {
  if (container.dataset.tasksEventsBound === 'true') return;
  container.dataset.tasksEventsBound = 'true';

  container.addEventListener('change', (e) => {
    if (e.target.id === 'tasks-filter-status') {
      viewState.filterStatus = e.target.value;
      renderTable();
    } else if (e.target.id === 'tasks-filter-module') {
      viewState.filterModule = e.target.value;
      renderTable();
    } else if (e.target.classList?.contains('tasks-row-check')) {
      const id = e.target.dataset.runId;
      if (e.target.checked) viewState.selected.add(id);
      else viewState.selected.delete(id);
      const selEl = document.getElementById('tasks-selected-count');
      if (selEl) selEl.textContent = t('tasks.selected_label', { n: viewState.selected.size });
    } else if (e.target.id === 'tasks-select-all') {
      const runs = filteredRuns();
      if (e.target.checked) {
        runs.forEach(r => {
          if (r.status !== 'Running' && r.status !== 'Pending') viewState.selected.add(r.id);
        });
      } else {
        runs.forEach(r => viewState.selected.delete(r.id));
      }
      renderTable();
    }
  });

  container.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    switch (btn.dataset.act) {
      case 'tasks-refresh':
        loadAll();
        break;
      case 'delete-run-in-tasks': {
        const runId = btn.dataset.runId;
        // Check for assets this run produced that would become orphans.
        let orphans = [];
        try { orphans = (await assetsApi.orphansForRun(runId)) || []; } catch { orphans = []; }
        const orphanNote = orphans.length
          ? '\n\n' + t('tasks.orphan_warning', { n: orphans.length })
          : '';
        const ok = await confirmModal({
          title: t('tasks.delete_confirm_title'),
          message: t('tasks.delete_confirm_message', { runId }) + orphanNote,
          okLabel: t('tasks.delete_run'),
        });
        if (!ok) return;
        try {
          await modulesApi.deleteRun(runId);
          showToast({ title: t('tasks.deleted_toast_title'), message: runId });
        } catch (err) {
          alertModal({ title: t('status.error_prefix'), message: String(err) });
          return;
        }
        await loadAll();
        break;
      }
      case 'tasks-delete-selected': {
        if (viewState.selected.size === 0) return;
        const n = viewState.selected.size;
        const ok = await confirmModal({
          title: t('tasks.delete_selected_title'),
          message: t('tasks.delete_selected_message', { n }),
          okLabel: t('tasks.delete_run'),
        });
        if (!ok) return;
        const ids = [...viewState.selected];
        let deleted = 0;
        const errors = [];
        for (const id of ids) {
          try { await modulesApi.deleteRun(id); deleted++; }
          catch (err) { errors.push(`${id}: ${err}`); }
        }
        viewState.selected.clear();
        showToast({
          title: t('tasks.deleted_toast_title'),
          message: t('tasks.deleted_count', { n: deleted }),
        });
        if (errors.length) {
          alertModal({ title: t('status.error_prefix'), message: errors.join('\n') });
        }
        await loadAll();
        break;
      }
      case 'tasks-clear-failed':
      case 'tasks-clear-cancelled':
      case 'tasks-clear-done': {
        const statusMap = {
          'tasks-clear-failed': ['Failed'],
          'tasks-clear-cancelled': ['Cancelled'],
          'tasks-clear-done': ['Done'],
        };
        const statuses = statusMap[btn.dataset.act];
        const matching = viewState.runs.filter(r => statuses.includes(r.status));
        if (matching.length === 0) {
          showToast({ title: t('tasks.nothing_to_clear'), message: '' });
          return;
        }
        const ok = await confirmModal({
          title: t('tasks.bulk_delete_title'),
          message: t('tasks.bulk_delete_message', {
            n: matching.length,
            statuses: statuses.map(s => t(`tasks.status_${s.toLowerCase()}`)).join(' / '),
            size: formatBytes(matching.reduce((acc, r) => acc + (viewState.sizes[r.id] || 0), 0)),
          }),
          okLabel: t('tasks.delete_run'),
        });
        if (!ok) return;
        try {
          const result = await modulesApi.clearRuns({ statuses });
          showToast({
            title: t('tasks.deleted_toast_title'),
            message: t('tasks.deleted_count', { n: result?.deleted ?? 0 }),
          });
          if (result?.errors?.length) {
            alertModal({ title: t('status.error_prefix'), message: result.errors.join('\n') });
          }
        } catch (err) {
          alertModal({ title: t('status.error_prefix'), message: String(err) });
        }
        await loadAll();
        break;
      }
    }
  });
}

export function renderTasksView(container) {
  const statusOpts = ['all', ...STATUS_OPTIONS].map(s => {
    const label = s === 'all' ? t('tasks.all_statuses') : t(`tasks.status_${s.toLowerCase()}`);
    const selected = viewState.filterStatus === s ? 'selected' : '';
    return `<option value="${s}" ${selected}>${escapeHtml(label)}</option>`;
  }).join('');

  container.innerHTML = `
    <div class="module-view tasks-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(92,112,128,0.12); color: #5c7080;">
          <i data-lucide="list-checks"></i>
        </div>
        <div>
          <h1 class="module-title">${escapeHtml(t('tasks.title'))}</h1>
          <p class="module-desc">${escapeHtml(t('tasks.subtitle'))}</p>
        </div>
      </div>

      <div class="card tasks-toolbar">
        <div class="tasks-toolbar-row">
          <label class="tasks-filter">
            <span>${escapeHtml(t('tasks.filter_status'))}</span>
            <select id="tasks-filter-status">${statusOpts}</select>
          </label>
          <label class="tasks-filter">
            <span>${escapeHtml(t('tasks.filter_module'))}</span>
            <select id="tasks-filter-module"></select>
          </label>
          <button type="button" class="btn btn-secondary" data-act="tasks-refresh">
            <i data-lucide="refresh-cw"></i> ${escapeHtml(t('common.refresh'))}
          </button>
          <div class="tasks-toolbar-spacer"></div>
          <button type="button" class="btn btn-secondary" data-act="tasks-clear-done">
            <i data-lucide="check-circle-2"></i> ${escapeHtml(t('tasks.clear_done'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="tasks-clear-failed">
            <i data-lucide="alert-triangle"></i> ${escapeHtml(t('tasks.clear_failed'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="tasks-clear-cancelled">
            <i data-lucide="ban"></i> ${escapeHtml(t('tasks.clear_cancelled'))}
          </button>
          <button type="button" class="btn btn-danger" data-act="tasks-delete-selected">
            <i data-lucide="trash-2"></i> ${escapeHtml(t('tasks.delete_selected'))}
          </button>
        </div>
        <div class="tasks-summary">
          <span id="tasks-count"></span>
          <span id="tasks-total-size"></span>
          <span id="tasks-selected-count"></span>
        </div>
      </div>

      <div class="card tasks-table-card">
        <table class="tasks-table">
          <thead>
            <tr>
              <th class="tasks-col-check">
                <input type="checkbox" id="tasks-select-all" aria-label="${escapeHtml(t('tasks.select_all'))}"/>
              </th>
              <th>${escapeHtml(t('tasks.col_id'))}</th>
              <th>${escapeHtml(t('tasks.col_module'))}</th>
              <th>${escapeHtml(t('tasks.col_status'))}</th>
              <th>${escapeHtml(t('tasks.col_size'))}</th>
              <th>${escapeHtml(t('tasks.col_lineage'))}</th>
              <th>${escapeHtml(t('tasks.col_time'))}</th>
              <th>${escapeHtml(t('tasks.col_actions'))}</th>
            </tr>
          </thead>
          <tbody id="tasks-table-body">
            <tr><td colspan="8">${escapeHtml(t('common.loading'))}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  `;

  bindEvents(container);

  // Wire a one-shot refresh when another view deletes a run.
  if (viewState.listener) window.removeEventListener('run-record-changed', viewState.listener);
  viewState.listener = () => loadAll();
  window.addEventListener('run-record-changed', viewState.listener);

  loadAll();
  if (window.lucide) window.lucide.createIcons();
}
