import { MODULES } from './constants.js';

export const state = {
  currentView: 'dashboard',
  files: {},
  pipelineStatus: {},
  projectOpen: false,
  projectName: '',
  logsByRun: {},
  runIdToModule: {},
  prefill: {},
};

MODULES.forEach(m => {
  state.files[m.id] = [];
  state.pipelineStatus[m.id] = 'idle';
});
