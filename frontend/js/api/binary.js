import { api } from '../core/tauri.js';

export const binaryApi = {
  getPaths() {
    return api.invoke('get_binary_paths');
  },
  setPath(name, path) {
    return api.invoke('set_binary_path', { name, path });
  },
  clearPath(name) {
    return api.invoke('clear_binary_path', { name });
  },
};
