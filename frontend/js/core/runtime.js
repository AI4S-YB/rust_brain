import { api } from './tauri.js';
import { t } from './i18n-helpers.js';
import { state } from './state.js';
import { MODULES } from './constants.js';
import { appendRunLog } from '../ui/log-panel.js';
import { loadRunsForView } from '../modules/run-result.js';
import { clearModuleRunStateByRunId, getBusyTaskCount } from './run-controls.js';

function refreshRunsForRun(runId) {
  if (!runId) return;
  const viewId = state.runIdToModule[runId];
  if (!viewId) return;
  const mod = MODULES.find(m => m.id === viewId);
  const backendId = mod?.backend || viewId;
  const containerId = `${viewId}-runs`;
  if (document.getElementById(containerId)) {
    loadRunsForView(backendId, containerId);
  }
  return viewId;
}

export function installRuntimeListeners() {
  api.listen('run-progress', event => {
    const st = document.getElementById('statusText');
    if (st) st.textContent = event.payload?.message || (t('status.running_prefix') + '…');
  });

  api.listen('run-completed', event => {
    const st = document.getElementById('statusText');
    clearModuleRunStateByRunId(event.payload?.runId);
    if (st) {
      st.textContent = getBusyTaskCount() > 0
        ? `${t('status.running_prefix')}…`
        : t('status.ready');
    }
    const viewId = refreshRunsForRun(event.payload?.runId);
    if (viewId) {
      const badge = document.querySelector(`.nav-item[data-view="${viewId}"] .nav-badge`);
      if (badge) { badge.className = 'nav-badge done'; badge.textContent = t('badge.done'); }
    }
  });

  api.listen('run-failed', event => {
    const err = event.payload?.error || t('status.run_failed');
    const st = document.getElementById('statusText');
    clearModuleRunStateByRunId(event.payload?.runId);
    if (st) st.textContent = `${t('status.error_prefix')}: ${err}`;
    console.error('[run-failed]', event.payload);
    const viewId = refreshRunsForRun(event.payload?.runId);
    if (viewId) {
      const badge = document.querySelector(`.nav-item[data-view="${viewId}"] .nav-badge`);
      if (badge) { badge.className = 'nav-badge'; badge.textContent = ''; }
    }
  });

  if (window.__TAURI__?.event) {
    window.__TAURI__.event.listen('run-log', (e) => {
      const { runId, line, stream } = e.payload || {};
      if (runId) appendRunLog(runId, line, stream);
    });
  }
}
