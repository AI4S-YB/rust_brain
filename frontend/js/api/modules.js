import { api } from '../core/tauri.js';

export const modulesApi = {
  validate(moduleId, params) {
    return api.invoke('validate_params', { moduleId, params });
  },
  run(moduleId, params, { inputsUsed = null, assetsUsed = null } = {}) {
    return api.invoke('run_module', {
      moduleId,
      params,
      inputsUsed,
      assetsUsed,
    });
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
  deleteRun(runId) {
    return api.invoke('delete_run', { runId });
  },
  clearRuns({ moduleId = null, statuses = null } = {}) {
    return api.invoke('clear_runs', { moduleId, statuses });
  },
  getRunSizes(runIds = null) {
    return api.invoke('get_run_sizes', { runIds });
  },
  listModules() {
    return api.invoke('list_modules');
  },
  getPluginManifest(id) {
    return api.invoke('get_plugin_manifest', { id });
  },
};
