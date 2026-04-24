import { api } from '../core/tauri.js';

export const inputsApi = {
  list() {
    return api.invoke('list_inputs');
  },
  register(path, opts = {}) {
    return api.invoke('register_input', {
      path,
      kind: opts.kind ?? null,
      displayName: opts.displayName ?? null,
    });
  },
  registerBatch(paths) {
    return api.invoke('register_inputs_batch', { paths });
  },
  update(id, patch) {
    return api.invoke('update_input', { id, patch });
  },
  delete(id) {
    return api.invoke('delete_input', { id });
  },
  scan() {
    return api.invoke('scan_inputs');
  },
};
