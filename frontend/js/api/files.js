import { api } from '../core/tauri.js';

export const filesApi = {
  selectFiles({ multiple = true, accept } = {}) {
    return api.invoke('select_files', { multiple, accept });
  },
  selectDirectory() {
    return api.invoke('select_directory', {});
  },
  readTablePreview(path, { maxRows = 50, maxCols = 10, hasHeader = true } = {}) {
    return api.invoke('read_table_preview', { path, maxRows, maxCols, hasHeader });
  },
};
