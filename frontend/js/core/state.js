import { MODULES } from './constants.js';

export const state = {
  currentView: 'dashboard',
  files: {},
  pipelineStatus: {},
  projectOpen: false,
  projectName: '',
  logsByRun: {},
  runIdToModule: {},
  // Insertion order of runIds, maintained alongside runIdToModule so
  // run-controls can evict the oldest entries when the map grows past
  // RUN_ID_HISTORY_CAP.
  runIdHistory: [],
  activeRunByModule: {},
  pendingRunByModule: {},
  cancelRequestedByModule: {},
  // Terminal run-completed/run-failed events received before the frontend had
  // time to register the runId (race: backend can fail validation and emit
  // before `run_module` IPC even returns). Consumed by registerStartedRun.
  pendingTerminalByRunId: {},
  prefill: {},
};

MODULES.forEach(m => {
  state.files[m.id] = [];
  state.pipelineStatus[m.id] = 'idle';
});

// Clear per-project runtime state. Called when a new project is opened so
// registry pickers, drop-zones, and run-state maps don't leak entries from
// the previous project. Project identity (projectOpen / projectName) is
// updated by the caller since the new values are known at that site.
export function resetProjectState() {
  Object.keys(state.files).forEach(k => { state.files[k] = []; });
  Object.keys(state.pipelineStatus).forEach(k => { state.pipelineStatus[k] = 'idle'; });
  state.logsByRun = {};
  state.runIdToModule = {};
  state.runIdHistory.length = 0;
  state.activeRunByModule = {};
  state.pendingRunByModule = {};
  state.cancelRequestedByModule = {};
  state.pendingTerminalByRunId = {};
  state.prefill = {};
}
