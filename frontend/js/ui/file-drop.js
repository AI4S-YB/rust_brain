import { state } from '../core/state.js';

export function fmtSize(b) {
  if (!b) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(b) / Math.log(1024));
  return `${(b / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${u[i]}`;
}

export function handleFileDrop(zone, fileList) {
  const mid = zone.dataset.module;
  if (!mid || !state.files[mid]) state.files[mid] = [];
  Array.from(fileList).forEach(f => {
    if (!state.files[mid]) state.files[mid] = [];
    state.files[mid].push({ name: f.name || f, size: f.size || 0 });
  });
  const list = document.getElementById(`${mid}-file-list`);
  if (list) renderFileList(list, mid);
}

export function renderFileList(el, mid) {
  const files = state.files[mid] || [];
  el.innerHTML = files.map((f, i) => `
    <div class="file-item">
      <i data-lucide="file" style="width:14px;height:14px;color:var(--text-muted);flex-shrink:0"></i>
      <span class="file-item-name" title="${f.name}">${f.name}</span>
      <span class="file-item-size">${fmtSize(f.size)}</span>
      <span class="file-item-remove" data-module="${mid}" data-index="${i}"><i data-lucide="x"></i></span>
    </div>`).join('');
  if (window.lucide) window.lucide.createIcons();
  el.querySelectorAll('.file-item-remove').forEach(btn => {
    btn.addEventListener('click', e => {
      e.stopPropagation();
      state.files[btn.dataset.module].splice(parseInt(btn.dataset.index), 1);
      renderFileList(el, btn.dataset.module);
    });
  });
}
