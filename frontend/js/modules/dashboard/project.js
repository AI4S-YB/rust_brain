import { state } from '../../core/state.js';
import { projectApi } from '../../api/project.js';
import { filesApi } from '../../api/files.js';
import { projectNewModal } from '../../ui/project-new-modal.js';
import { alertModal } from '../../ui/modal.js';

function setProjectUI(name) {
  state.projectOpen = true;
  state.projectName = name;
  const headerEl = document.getElementById('projectName');
  if (headerEl) headerEl.textContent = name;
  const dashEl = document.getElementById('dash-proj-name');
  if (dashEl) dashEl.textContent = name;
}

export async function projectNew() {
  const picked = await projectNewModal();
  if (!picked) return;
  const { name, default_view } = picked;
  try {
    const dir = await filesApi.selectDirectory();
    const info = await projectApi.create({ name, dir, defaultView: default_view });
    setProjectUI(name);
    const dv = (info && info.default_view) || default_view;
    if (dv === 'ai') location.hash = '#chat';
  } catch (err) {
    console.warn('[projectNew] invoke failed, using local fallback:', err);
    setProjectUI(name);
  }
}

export async function projectOpen() {
  let dir;
  try {
    dir = await filesApi.selectDirectory();
  } catch (err) {
    console.warn('[projectOpen] selectDirectory failed:', err);
    return;
  }
  if (!dir) return;
  try {
    const result = await projectApi.open(dir);
    const name = (result && result.name) ? result.name : dir || 'Opened Project';
    setProjectUI(name);
    if (result && result.default_view === 'ai') location.hash = '#chat';
  } catch (err) {
    console.warn('[projectOpen] invoke failed:', err);
    alertModal({ title: 'Error', message: 'Failed to open project: ' + err });
  }
}
