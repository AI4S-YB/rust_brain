import { state } from '../core/state.js';
import { t } from '../core/i18n-helpers.js';
import { inputsApi } from '../api/inputs.js';
import { invalidatePickerCache } from './registry-picker.js';

export function fmtSize(b) {
  if (!b) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(b) / Math.log(1024));
  return `${(b / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${u[i]}`;
}

function refreshFileBadges(mid) {
  const n = (state.files[mid] || []).length;
  document.querySelectorAll(`[data-files-count="${mid}"]`).forEach(el => {
    const key = el.dataset.filesCountKey;
    el.textContent = key ? t(key, { n }) : String(n);
  });
}

export function handleFileDrop(zone, fileList) {
  const mid = zone.dataset.module;
  if (!mid) return;
  if (!state.files[mid]) state.files[mid] = [];
  const single = zone.hasAttribute('data-param-single');
  const incoming = Array.from(fileList).map(f => {
    const name = f.name || f;
    const path = f.path || (typeof f === 'string' ? f : name);
    return { name, size: f.size || 0, path };
  });
  if (single) {
    state.files[mid] = incoming.slice(-1);
  } else {
    state.files[mid].push(...incoming);
  }
  const list = document.getElementById(`${mid}-file-list`);
  if (list) renderFileList(list, mid);
  refreshFileBadges(mid);
  maybeAutoRegister(incoming);
}

/**
 * Best-effort: register each incoming file as a project Input so it shows up
 * in the registry picker next time. Only runs for absolute paths — real
 * drag-from-OS events where the browser hides the path are silently skipped.
 */
async function maybeAutoRegister(incoming) {
  const absolutes = incoming
    .map(f => f.path)
    .filter(p => typeof p === 'string' && (p.startsWith('/') || /^[a-zA-Z]:[\\/]/.test(p)));
  if (absolutes.length === 0) return;
  let anyOk = false;
  for (const p of absolutes) {
    try {
      await inputsApi.register(p);
      anyOk = true;
    } catch { /* silent */ }
  }
  if (anyOk) invalidatePickerCache();
}

export function renderFileList(el, mid) {
  const files = state.files[mid] || [];
  el.innerHTML = files.map((f, i) => `
    <div class="file-item">
      <i data-lucide="file" style="width:14px;height:14px;color:var(--text-muted);flex-shrink:0"></i>
      <span class="file-item-name" title="${f.path || f.name}">${f.name}</span>
      ${f.size ? `<span class="file-item-size">${fmtSize(f.size)}</span>` : ''}
      <span class="file-item-remove" data-module="${mid}" data-index="${i}"><i data-lucide="x"></i></span>
    </div>`).join('');
  if (window.lucide) window.lucide.createIcons();
  el.querySelectorAll('.file-item-remove').forEach(btn => {
    btn.addEventListener('click', e => {
      e.stopPropagation();
      const m = btn.dataset.module;
      state.files[m].splice(parseInt(btn.dataset.index), 1);
      renderFileList(el, m);
      refreshFileBadges(m);
    });
  });
}
