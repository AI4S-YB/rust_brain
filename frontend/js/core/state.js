import { MODULES } from './constants.js';

export const state = {
  currentView: 'dashboard',
  files: {},
  pipelineStatus: {},
  projectOpen: false,
  projectName: '',
  logsByRun: {},
  runIdToModule: {},
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
