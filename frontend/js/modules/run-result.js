import { t } from '../core/i18n-helpers.js';
import { modulesApi } from '../api/modules.js';
import { escapeHtml } from '../ui/escape.js';
import { MODULES } from '../core/constants.js';
import { renderGffConvertResult } from './gff-convert/result.js';
import { renderQcResult } from './qc/result.js';
import { renderStarAlignResult } from './star-align/result.js';
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
    case 'gff_convert': html = renderGffConvertResult(result, runId); break;
    case 'star_align': html = renderStarAlignResult(result, runId); break;
    case 'star_index': html = `<pre>${escapeHtml(JSON.stringify(result.summary, null, 2))}</pre>`; break;
    default: html = `<pre>${escapeHtml(JSON.stringify(result, null, 2))}</pre>`; break;
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

export async function loadRunsForView(moduleId, containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  container.dataset.runsModuleId = moduleId;
  try {
    const runs = await modulesApi.listRuns(moduleId);
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
      const size = sizes[run.id];
      const sizeLabel = size != null ? ` — ${formatBytes(size)}` : '';
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
              ${t('status.run_label')} ${escapeHtml(run.id)} &mdash; ${escapeHtml(status)}
              ${ts ? ' (' + escapeHtml(ts) + ')' : ''}${sizeLabel}
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
