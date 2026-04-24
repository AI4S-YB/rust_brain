import { MAX_COMPUTE_LOAD, MODULES, RUN_TASKS } from './constants.js';
import { t, navKey } from './i18n-helpers.js';
import { state } from './state.js';
import { modulesApi } from '../api/modules.js';
import { loadRunsForView } from '../modules/run-result.js';
import { showToast } from '../ui/modal.js';

function getTaskConfig(moduleId) {
  return RUN_TASKS[moduleId] || null;
}

const runStatusMonitors = new Map();
const TERMINAL_STATUSES = new Set(['Done', 'Failed', 'Cancelled']);

// FIFO cap on state.runIdToModule so long sessions don't grow it unbounded.
// log-panel / deleteRunWithConfirm look entries up by runId, so we keep a
// generous window instead of deleting on run completion.
const RUN_ID_HISTORY_CAP = 256;

function recordRunIdMapping(runId, moduleId) {
  if (state.runIdToModule[runId]) {
    state.runIdToModule[runId] = moduleId;
    return;
  }
  state.runIdToModule[runId] = moduleId;
  state.runIdHistory.push(runId);
  while (state.runIdHistory.length > RUN_ID_HISTORY_CAP) {
    const oldest = state.runIdHistory.shift();
    if (oldest && !Object.values(state.activeRunByModule).includes(oldest)) {
      delete state.runIdToModule[oldest];
    }
  }
}

function backendIdForModule(moduleId) {
  return getTaskConfig(moduleId)?.backend
    || MODULES.find(m => m.id === moduleId || m.view_id === moduleId)?.backend
    || moduleId;
}

function getBusyModuleIds() {
  const ids = new Set();
  Object.entries(state.pendingRunByModule).forEach(([moduleId, pending]) => {
    if (pending) ids.add(moduleId);
  });
  Object.entries(state.activeRunByModule).forEach(([moduleId, runId]) => {
    if (runId) ids.add(moduleId);
  });
  return ids;
}

export function getBusyTaskCount() {
  return getBusyModuleIds().size;
}

export function getCurrentComputeLoad() {
  let total = 0;
  getBusyModuleIds().forEach(moduleId => {
    total += getTaskConfig(moduleId)?.computeCost || 0;
  });
  return total;
}

function setButtonContent(btn, icon, labelKey) {
  btn.innerHTML = `<i data-lucide="${icon}"></i> ${t(labelKey)}`;
}

function updateJobStatusText() {
  const jobStatus = document.getElementById('jobStatus');
  if (!jobStatus) return;
  const count = getBusyTaskCount();
  if (count === 0) {
    jobStatus.textContent = t('status.no_jobs');
  } else {
    jobStatus.textContent = t('status.jobs_active', { n: count });
  }
}

function refreshRunsForModule(moduleId) {
  const backendId = backendIdForModule(moduleId);
  const containerId = `${moduleId}-runs`;
  if (backendId && document.getElementById(containerId)) {
    loadRunsForView(backendId, containerId);
  }
}

function stopRunStatusMonitor(runId) {
  const interval = runStatusMonitors.get(runId);
  if (!interval) return;
  clearInterval(interval);
  runStatusMonitors.delete(runId);
}

function startRunStatusMonitor(moduleId, runId) {
  stopRunStatusMonitor(runId);
  const backendId = backendIdForModule(moduleId);
  if (!backendId) return;

  const check = async () => {
    if (state.activeRunByModule[moduleId] !== runId) {
      stopRunStatusMonitor(runId);
      return;
    }
    try {
      const runs = await modulesApi.listRuns(backendId);
      const run = (runs || []).find(r => r.id === runId);
      if (!run || !TERMINAL_STATUSES.has(run.status)) return;
      stopRunStatusMonitor(runId);
      const { applyTerminalRunRecord } = await import('./runtime.js');
      applyTerminalRunRecord(run);
    } catch (err) {
      console.warn(`[run-monitor] failed to refresh ${runId}:`, err);
    }
  };

  runStatusMonitors.set(runId, setInterval(check, 2000));
  setTimeout(check, 500);
}

export function canStartModuleRun(moduleId) {
  const cfg = getTaskConfig(moduleId);
  if (!cfg) return false;
  return getCurrentComputeLoad() + cfg.computeCost <= MAX_COMPUTE_LOAD;
}

export function isModuleBusy(moduleId) {
  return !!state.pendingRunByModule[moduleId] || !!state.activeRunByModule[moduleId];
}

export function showComputeBudgetToast(moduleId) {
  const needed = getTaskConfig(moduleId)?.computeCost || 0;
  showToast({
    title: t('status.compute_budget_title'),
    message: t('status.compute_budget_message', {
      module: t(navKey(moduleId)),
      current: getCurrentComputeLoad(),
      max: MAX_COMPUTE_LOAD,
      needed,
    }),
  });
}

export function syncRunButtons(root = document) {
  const scope = root?.querySelectorAll ? root : document;
  scope.querySelectorAll('[data-run-button][data-mod]').forEach(btn => {
    const moduleId = btn.dataset.mod;
    const isPending = !!state.pendingRunByModule[moduleId];
    const isRunning = !!state.activeRunByModule[moduleId];
    const isBusy = isPending || isRunning;
    const isCancelling = !!state.cancelRequestedByModule[moduleId];

    if (isBusy) {
      btn.type = 'button';
      btn.dataset.act = 'cancel-run';
      btn.disabled = isCancelling;
      btn.title = '';
      setButtonContent(btn, 'x', 'common.cancel');
      return;
    }

    btn.type = btn.dataset.runButtonType || 'button';
    if (btn.dataset.runButtonAct) btn.dataset.act = btn.dataset.runButtonAct;
    else btn.removeAttribute('data-act');

    const blocked = !canStartModuleRun(moduleId);
    btn.disabled = blocked;
    btn.title = blocked
      ? t('status.compute_budget_button', {
          current: getCurrentComputeLoad(),
          max: MAX_COMPUTE_LOAD,
          needed: getTaskConfig(moduleId)?.computeCost || 0,
        })
      : '';
    setButtonContent(btn, btn.dataset.runIcon || 'play', btn.dataset.runLabelKey || 'common.run');
  });

  if (window.lucide) window.lucide.createIcons();
  updateJobStatusText();
}

export function markModuleRunPending(moduleId) {
  state.pendingRunByModule[moduleId] = true;
  delete state.cancelRequestedByModule[moduleId];
  syncRunButtons();
}

export function clearModuleRunState(moduleId) {
  const runId = state.activeRunByModule[moduleId];
  if (runId) stopRunStatusMonitor(runId);
  delete state.pendingRunByModule[moduleId];
  delete state.activeRunByModule[moduleId];
  delete state.cancelRequestedByModule[moduleId];
  syncRunButtons();
}

export function moduleIdForRunId(runId) {
  if (!runId) return null;
  if (state.runIdToModule[runId]) return state.runIdToModule[runId];
  for (const [moduleId, activeRunId] of Object.entries(state.activeRunByModule)) {
    if (activeRunId === runId) return moduleId;
  }
  return null;
}

export function onlyActiveRunId() {
  const active = Object.values(state.activeRunByModule).filter(Boolean);
  return active.length === 1 ? active[0] : null;
}

export function clearModuleRunStateByRunId(runId) {
  const moduleId = moduleIdForRunId(runId);
  if (!moduleId) return null;
  const activeRunId = state.activeRunByModule[moduleId];
  if (!activeRunId || activeRunId === runId) {
    stopRunStatusMonitor(runId);
    delete state.activeRunByModule[moduleId];
    delete state.pendingRunByModule[moduleId];
    delete state.cancelRequestedByModule[moduleId];
  }
  syncRunButtons();
  return moduleId;
}

export async function registerStartedRun(moduleId, runId) {
  recordRunIdMapping(runId, moduleId);
  delete state.pendingRunByModule[moduleId];
  state.activeRunByModule[moduleId] = runId;
  syncRunButtons();
  startRunStatusMonitor(moduleId, runId);

  // If a terminal event (run-completed/run-failed) arrived before we could
  // register the runId, replay it now so the button doesn't get stuck.
  const pending = state.pendingTerminalByRunId[runId];
  if (pending) {
    delete state.pendingTerminalByRunId[runId];
    const { applyRunCompleted, applyRunFailed } = await import('./runtime.js');
    if (pending.kind === 'completed') applyRunCompleted(runId);
    else applyRunFailed(runId, pending.error);
    return false;
  }

  if (state.cancelRequestedByModule[moduleId]) {
    await cancelModuleRun(moduleId);
    return false;
  }

  return true;
}

export async function cancelModuleRun(moduleId) {
  if (state.pendingRunByModule[moduleId] && !state.activeRunByModule[moduleId]) {
    state.cancelRequestedByModule[moduleId] = true;
    syncRunButtons();
    return;
  }

  const runId = state.activeRunByModule[moduleId];
  if (!runId) return;

  state.cancelRequestedByModule[moduleId] = true;
  syncRunButtons();

  try {
    await modulesApi.cancelRun(runId);
  } finally {
    clearModuleRunState(moduleId);
    refreshRunsForModule(moduleId);

    const statusText = document.getElementById('statusText');
    if (statusText) {
      statusText.textContent = getBusyTaskCount() > 0
        ? `${t('status.running_prefix')}…`
        : t('status.ready');
    }

    const badge = document.querySelector(`.nav-item[data-view="${moduleId}"] .nav-badge`);
    if (badge) {
      badge.className = 'nav-badge';
      badge.textContent = '';
    }
  }
}
