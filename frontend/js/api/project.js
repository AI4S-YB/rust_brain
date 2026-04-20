import { api } from '../core/tauri.js';

export const projectApi = {
  create({ name, dir, defaultView }) {
    return api.invoke('create_project', { name, dir, defaultView });
  },
  open(dir) {
    return api.invoke('open_project', { dir });
  },
};
