import { t } from '../core/i18n-helpers.js';
import { modulesApi } from '../api/modules.js';
import { escapeHtml } from '../ui/escape.js';
import { renderGffConvertResult } from './gff-convert/result.js';
import { renderStarAlignResult } from './star-align/result.js';

export function renderRunResultHtml(moduleId, result, runId) {
  let html = '';
  switch (moduleId) {
    case 'gff_convert': html = renderGffConvertResult(result, runId); break;
    case 'star_align': html = renderStarAlignResult(result, runId); break;
    case 'star_index': html = `<pre>${escapeHtml(JSON.stringify(result.summary, null, 2))}</pre>`; break;
    default: html = `<pre>${escapeHtml(JSON.stringify(result, null, 2))}</pre>`; break;
  }
  return html;
}

export async function loadRunsForView(moduleId, containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  try {
    const runs = await modulesApi.listRuns(moduleId);
    if (!runs || runs.length === 0) {
      container.innerHTML = `<p><em>${t('status.no_runs')}</em></p>`;
      return;
    }
    container.innerHTML = runs.map(run => {
      const status = run.status || 'unknown';
      const ts = run.finished_at || run.started_at || '';
      const resultHtml = (status === 'Done' && run.result)
        ? renderRunResultHtml(moduleId, run.result, run.id)
        : `<p><em>${t('status.status_label')}: ${status}</em></p>`;
      return `<details open><summary>${t('status.run_label')} ${run.id} &mdash; ${status} ${ts ? '(' + ts + ')' : ''}</summary>${resultHtml}</details>`;
    }).join('');
  } catch (err) {
    container.innerHTML = `<p><em>${t('status.load_runs_failed')}: ${err}</em></p>`;
  }
}
