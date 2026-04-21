import { api } from '../core/tauri.js';

export const pluginsApi = {
  listStatus() {
    return api.invoke('list_plugin_status');
  },
  reload() {
    return api.invoke('reload_plugins');
  },
};
