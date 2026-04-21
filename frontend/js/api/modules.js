import { api } from '../core/tauri.js';

export const modulesApi = {
  validate(moduleId, params) {
    return api.invoke('validate_params', { moduleId, params });
  },
  run(moduleId, params) {
    return api.invoke('run_module', { moduleId, params });
  },
  listRuns(moduleId) {
    return api.invoke('list_runs', { moduleId });
  },
  getResult(moduleId, runId) {
    return api.invoke('get_result', { moduleId, runId });
  },
  cancelRun(runId) {
    return api.invoke('cancel_run', { runId });
  },
  listModules() {
    return api.invoke('list_modules');
  },
  getPluginManifest(id) {
    return api.invoke('get_plugin_manifest', { id });
  },
};
