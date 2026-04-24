import { t } from '../core/i18n-helpers.js';
import { modulesApi } from '../api/modules.js';
import { escapeHtml } from '../ui/escape.js';
import { MODULES } from '../core/constants.js';
import { state } from '../core/state.js';
import { renderGffConvertResult } from './gff-convert/result.js';
import { renderCountsMergeResult } from './counts-merge/result.js';
import { renderQcResult } from './qc/result.js';
import { renderStarAlignResult } from './star-align/result.js';
import { renderTrimmingResult } from './trimming/result.js';
import { renderPluginResult } from './plugin/result.js';
import { confirmModal, alertModal } from '../ui/modal.js';

export function renderRunResultHtml(moduleId, result, runId) {
  // Plugin modules use the generic renderer.
  const mod = MODULES.find(m => m.id === moduleId || m.view_id === moduleId);
  if (mod && mod.has_native_view === false) {
    return renderPluginResult(result, runId);
  }

  let html = '';
  switch (moduleId) {
    case 'qc': html = renderQcResult(result, runId); break;
    case 'trimming': html = renderTrimmingResult(result, runId); break;
    case 'gff_convert': html = renderGffConvertResult(result, runId); break;
    case 'star_align': html = renderStarAlignResult(result, runId); break;
    case 'counts_merge': html = renderCountsMergeResult(result, runId); break;
    default: html = renderGenericResult(result); break;
  }
  return html;
}

export function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(bytes)) return '';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let n = bytes / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n >= 10 ? 0 : 1)} ${units[i]}`;
}

function renderDeleteButton(runId, status) {
  const blocked = status === 'Running' || status === 'Pending';
  const titleKey = blocked ? 'tasks.delete_blocked_running' : 'tasks.delete_run';
  return `
    <button type="button"
            class="run-delete-btn"
            data-act="delete-run"
            data-run-id="${escapeHtml(runId)}"
            title="${escapeHtml(t(titleKey))}"
            ${blocked ? 'disabled' : ''}
            aria-label="${escapeHtml(t('tasks.delete_run'))}">
      <i data-lucide="trash-2"></i>
    </button>`;
}

function isTerminalStatus(status) {
  return status === 'Done' || status === 'Failed' || status === 'Cancelled';
}

function statusLabel(status) {
  const key = `tasks.status_${String(status || 'unknown').toLowerCase()}`;
  const label = t(key);
  return label === key ? (status || 'unknown') : label;
}

function formatRunTimestamp(ts) {
  if (!ts) return '';
  const normalized = String(ts).replace(/(\.\d{3})\d+(Z|[+-]\d\d:\d\d)?$/, '$1$2');
  const date = new Date(normalized);
  if (Number.isNaN(date.getTime())) return ts;
  try {
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    }).format(date);
  } catch {
    return ts;
  }
}

function isPrimitive(value) {
  return value == null || ['string', 'number', 'boolean'].includes(typeof value);
}

function humanizeKey(key) {
  return String(key)
    .replace(/_/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase());
}

function genericValueHtml(key, value) {
  if (value == null || value === '') return '-';
  if (typeof value === 'boolean') return value ? t('common.yes') : t('common.no');
  if (typeof value === 'number' && /bytes|size/i.test(key)) return formatBytes(value);
  return escapeHtml(String(value));
}

function renderGenericResult(result) {
  const summary = result?.summary && typeof result.summary === 'object' ? result.summary : {};
  const outputFiles = Array.isArray(result?.output_files) ? result.output_files : [];
  const primitiveRows = Object.entries(summary).filter(([, value]) => isPrimitive(value));
  const detailRows = Object.entries(summary).filter(([, value]) => !isPrimitive(value));
  const summaryHtml = primitiveRows.length ? `
    <dl class="result-kv">
      ${primitiveRows.map(([key, value]) => `
        <dt>${escapeHtml(humanizeKey(key))}</dt>
        <dd class="${/path|dir|file/i.test(key) ? 'path' : ''}" title="${escapeHtml(String(value ?? ''))}">
          ${genericValueHtml(key, value)}
        </dd>`).join('')}
    </dl>` : '';
  const outputsHtml = outputFiles.length ? `
    <h3>${escapeHtml(t('common.output_files'))}</h3>
    <ul class="run-result-list">
      ${outputFiles.map(path => `<li class="path" title="${escapeHtml(path)}">${escapeHtml(path)}</li>`).join('')}
    </ul>` : '';
  const detailsHtml = detailRows.map(([key, value]) => `
    <details class="result-json-details">
      <summary>${escapeHtml(humanizeKey(key))}</summary>
      <pre>${escapeHtml(JSON.stringify(value, null, 2))}</pre>
    </details>`).join('');
  const logHtml = result?.log ? `
    <details class="log-panel">
      <summary>${escapeHtml(t('common.log_panel'))}</summary>
      <pre>${escapeHtml(result.log)}</pre>
    </details>` : '';

  return `
    <div class="run-result-card generic-result-card">
      <h3>${escapeHtml(t('common.summary'))}</h3>
      ${summaryHtml || `<p><em>${escapeHtml(t('common.no_summary'))}</em></p>`}
      ${outputsHtml}
      ${detailsHtml}
      ${logHtml}
    </div>`;
}

async function reconcileRunUiState(runs) {
  const terminalRuns = (runs || []).filter(run => run?.id && isTerminalStatus(run.status));
  if (!terminalRuns.length) return;

  const { applyTerminalRunRecord } = await import('../core/runtime.js');
  for (const run of terminalRuns) {
    if (!Object.values(state.activeRunByModule).includes(run.id)) continue;
    applyTerminalRunRecord(run, { refresh: false });
  }
}

export async function loadRunsForView(moduleId, containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  container.dataset.runsModuleId = moduleId;
  try {
    const runs = await modulesApi.listRuns(moduleId);
    await reconcileRunUiState(runs);
    if (!runs || runs.length === 0) {
      container.innerHTML = `<p><em>${t('status.no_runs')}</em></p>`;
      return;
    }
    const ids = runs.map(r => r.id);
    let sizes = {};
    try { sizes = await modulesApi.getRunSizes(ids) || {}; } catch { sizes = {}; }

    container.innerHTML = runs.map(run => {
      const status = run.status || 'unknown';
      const ts = run.finished_at || run.started_at || '';
      const tsLabel = formatRunTimestamp(ts);
      const size = sizes[run.id];
      const sizeLabel = size != null
        ? `<span class="run-entry-size">${escapeHtml(formatBytes(size))}</span>`
        : '';
      let bodyHtml;
      if (status === 'Done' && run.result) {
        bodyHtml = renderRunResultHtml(moduleId, run.result, run.id);
      } else if (status === 'Failed' && run.error) {
        bodyHtml = `
          <p><em>${t('status.status_label')}: ${status}</em></p>
          <pre class="run-error-box">${escapeHtml(run.error)}</pre>
        `;
      } else {
        bodyHtml = `<p><em>${t('status.status_label')}: ${status}</em></p>`;
      }
      const deleteBtn = renderDeleteButton(run.id, status);
      return `
        <details open class="run-entry" data-run-id="${escapeHtml(run.id)}">
          <summary>
            <span class="run-entry-summary-text">
              ${escapeHtml(t('status.run_label'))} <code>${escapeHtml(run.id)}</code>
              <span class="status-pill status-${escapeHtml(status.toLowerCase())}">${escapeHtml(statusLabel(status))}</span>
              ${tsLabel ? '<span class="run-entry-time">' + escapeHtml(tsLabel) + '</span>' : ''}${sizeLabel}
            </span>
            ${deleteBtn}
          </summary>
          ${bodyHtml}
        </details>`;
    }).join('');
    if (window.lucide) window.lucide.createIcons();
  } catch (err) {
    container.innerHTML = `<p><em>${t('status.load_runs_failed')}: ${err}</em></p>`;
  }
}

/**
 * Delete a run record (and its on-disk directory) after user confirmation,
 * then refresh the container it belonged to. Exposed so events.js can invoke it.
 */
export async function deleteRunWithConfirm(runId, containerEl) {
  if (!runId) return;
  const ok = await confirmModal({
    title: t('tasks.delete_confirm_title'),
    message: t('tasks.delete_confirm_message', { runId }),
    okLabel: t('tasks.delete_run'),
  });
  if (!ok) return;
  try {
    await modulesApi.deleteRun(runId);
  } catch (err) {
    alertModal({
      title: t('status.error_prefix'),
      message: String(err),
    });
    return;
  }
  // Prefer refreshing the container the button lives in.
  const container = containerEl
    || document.querySelector(`[data-runs-module-id] [data-run-id="${CSS.escape(runId)}"]`)?.closest('[data-runs-module-id]')
    || null;
  if (container?.id && container.dataset.runsModuleId) {
    loadRunsForView(container.dataset.runsModuleId, container.id);
  }
  // Also notify the Tasks view if it's open.
  window.dispatchEvent(new CustomEvent('run-record-changed', { detail: { runId } }));
}
