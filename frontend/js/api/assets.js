import { api } from '../core/tauri.js';

export const assetsApi = {
  list() {
    return api.invoke('list_assets');
  },
  delete(id) {
    return api.invoke('delete_asset', { id });
  },
  orphansForRun(runId) {
    return api.invoke('orphan_assets_for_run', { runId });
  },
};
