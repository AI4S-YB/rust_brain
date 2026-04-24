import { api } from '../core/tauri.js';

export const samplesApi = {
  list() {
    return api.invoke('list_samples');
  },
  create(name, { group = null, condition = null, inputIds = [] } = {}) {
    return api.invoke('create_sample', {
      name,
      group,
      condition,
      inputIds,
    });
  },
  update(id, patch) {
    return api.invoke('update_sample', { id, patch });
  },
  delete(id) {
    return api.invoke('delete_sample', { id });
  },
  autoPair() {
    return api.invoke('auto_pair_samples');
  },
  previewAutoPair(patterns = []) {
    return api.invoke('preview_auto_pair_samples', { patterns });
  },
  importTsv(path) {
    return api.invoke('import_samples_from_tsv', { path });
  },
};
