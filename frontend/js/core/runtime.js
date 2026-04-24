import { api } from './tauri.js';
import { t } from './i18n-helpers.js';
import { state } from './state.js';
import { MODULES, RUN_TASKS } from './constants.js';
import { appendRunLog } from '../ui/log-panel.js';
import { loadRunsForView } from '../modules/run-result.js';
import {
  clearModuleRunStateByRunId,
  getBusyTaskCount,
  moduleIdForRunId,
  onlyActiveRunId,
} from './run-controls.js';
import { showToast } from '../ui/modal.js';

function runIdFromPayload(payload) {
  return payload?.runId
    || payload?.run_id
    || payload?.id
    || payload?.result?.runId
    || payload?.result?.run_id
    || null;
}

function runIdFromTerminalEvent(event) {
  return runIdFromPayload(event?.payload) || onlyActiveRunId();
}

function refreshRunsForRun(runId) {
  if (!runId) return;
  const viewId = state.runIdToModule[runId];
  if (!viewId) return;
  const mod = MODULES.find(m => m.id === viewId || m.view_id === viewId);
  const backendId = mod?.backend || RUN_TASKS[viewId]?.backend || viewId;
  const containerId = `${viewId}-runs`;
  if (document.getElementById(containerId)) {
    loadRunsForView(backendId, containerId);
  }
  return viewId;
}

export function applyRunCompleted(runId, { refresh = true } = {}) {
  const st = document.getElementById('statusText');
  clearModuleRunStateByRunId(runId);
  if (st) {
    st.textContent = getBusyTaskCount() > 0
      ? `${t('status.running_prefix')}…`
      : t('status.ready');
  }
  const viewId = refresh ? refreshRunsForRun(runId) : state.runIdToModule[runId];
  if (viewId) {
    const badge = document.querySelector(`.nav-item[data-view="${viewId}"] .nav-badge`);
    if (badge) { badge.className = 'nav-badge done'; badge.textContent = t('badge.done'); }
  }
}

export function applyRunFailed(runId, error, { refresh = true } = {}) {
  const err = error || t('status.run_failed');
  const st = document.getElementById('statusText');
  clearModuleRunStateByRunId(runId);
  if (st) st.textContent = `${t('status.error_prefix')}: ${err}`;
  console.error('[run-failed]', runId, err);
  showToast({
    title: t('status.run_failed'),
    message: err,
    duration: 6000,
  });
  const viewId = refresh ? refreshRunsForRun(runId) : state.runIdToModule[runId];
  if (viewId) {
    const badge = document.querySelector(`.nav-item[data-view="${viewId}"] .nav-badge`);
    if (badge) { badge.className = 'nav-badge'; badge.textContent = ''; }
  }
}

export function applyRunCancelled(runId, { refresh = true } = {}) {
  const st = document.getElementById('statusText');
  clearModuleRunStateByRunId(runId);
  if (st) {
    st.textContent = getBusyTaskCount() > 0
      ? `${t('status.running_prefix')}…`
      : t('status.ready');
  }
  const viewId = refresh ? refreshRunsForRun(runId) : state.runIdToModule[runId];
  if (viewId) {
    const badge = document.querySelector(`.nav-item[data-view="${viewId}"] .nav-badge`);
    if (badge) { badge.className = 'nav-badge'; badge.textContent = ''; }
  }
}

export function applyTerminalRunRecord(run, options = {}) {
  if (!run?.id) return;
  if (run.status === 'Done') {
    applyRunCompleted(run.id, options);
  } else if (run.status === 'Failed') {
    applyRunFailed(run.id, run.error, options);
  } else if (run.status === 'Cancelled') {
    applyRunCancelled(run.id, options);
  }
}

export function installRuntimeListeners() {
  api.listen('run-progress', event => {
    const st = document.getElementById('statusText');
    if (st) st.textContent = event.payload?.message || (t('status.running_prefix') + '…');
  });

  api.listen('run-completed', event => {
    const runId = runIdFromTerminalEvent(event);
    // Race: backend may emit the terminal before the frontend has had a chance
    // to register the runId in state.runIdToModule (happens when validation
    // fails synchronously inside module.run). Buffer and let registerStartedRun
    // replay it.
    if (runId && !moduleIdForRunId(runId)) {
      state.pendingTerminalByRunId[runId] = { kind: 'completed' };
      return;
    }
    applyRunCompleted(runId);
  });

  api.listen('run-failed', event => {
    const runId = runIdFromTerminalEvent(event);
    const err = event.payload?.error || event.payload?.message;
    if (runId && !moduleIdForRunId(runId)) {
      state.pendingTerminalByRunId[runId] = { kind: 'failed', error: err };
      return;
    }
    applyRunFailed(runId, err);
  });

  if (window.__TAURI__?.event) {
    window.__TAURI__.event.listen('run-log', (e) => {
      const runId = runIdFromPayload(e.payload);
      const { line, stream } = e.payload || {};
      if (runId) appendRunLog(runId, line, stream);
    });
  }
}
