import { MODULES } from './constants.js';
import { state } from './state.js';
import { t, navKey } from './i18n-helpers.js';
import { navigate } from './router.js';
import { modulesApi } from '../api/modules.js';
import { loadRunsForView } from '../modules/run-result.js';
import { runStartedToast } from '../ui/modal.js';

export function collectModuleParams() {
  const params = {};
  const root = document.querySelector('.module-view') || document.getElementById('content');
  if (!root) return params;

  root.querySelectorAll('input[data-param], select[data-param], textarea[data-param]').forEach(el => {
    const name = el.dataset.param;
    if (el.type === 'checkbox') {
      params[name] = el.checked;
    } else if (el.type === 'number') {
      if (el.value !== '') {
        const n = Number(el.value);
        if (!Number.isNaN(n)) params[name] = n;
      }
    } else {
      const v = el.value;
      if (v != null && v !== '') params[name] = v;
    }
  });

  root.querySelectorAll('.file-drop-zone[data-param]').forEach(zone => {
    const name = zone.dataset.param;
    const mid = zone.dataset.module;
    const paths = (state.files[mid] || []).map(f => f.path || f.name);
    if (paths.length === 0) return;
    if (zone.hasAttribute('data-param-single')) {
      params[name] = paths[0];
    } else {
      params[name] = paths;
    }
  });

  return params;
}

export async function runModule(id) {
  const mod = MODULES.find(m => m.id === id);
  if (!mod || !mod.backend) {
    console.warn(`[runModule] module '${id}' has no backend — skipped`);
    return;
  }
  const backendId = mod.backend;
  const st = document.getElementById('statusText');
  const js = document.getElementById('jobStatus');
  const displayName = t(navKey(mod.id));
  const badge = document.querySelector(`.nav-item[data-view="${id}"] .nav-badge`);
  const btn = document.querySelector(`.module-view [data-act="run-module"][data-mod="${id}"]`);
  if (btn) btn.disabled = true;

  const params = collectModuleParams();
  try {
    const errors = await modulesApi.validate(backendId, params);
    if (Array.isArray(errors) && errors.length > 0) {
      const msg = errors.map(e => `• ${e.field}: ${e.message}`).join('\n');
      alert(`${t('status.error_prefix')}:\n${msg}`);
      return;
    }

    if (st) st.textContent = `${t('status.running_prefix')} ${displayName}…`;
    if (js) js.textContent = t('status.one_job');
    if (badge) { badge.className = 'nav-badge running'; badge.textContent = t('badge.running'); }

    const runId = await modulesApi.run(backendId, params);
    if (runId) {
      state.runIdToModule[runId] = id;
      const containerId = `${id}-runs`;
      if (document.getElementById(containerId)) {
        loadRunsForView(backendId, containerId);
      }
      runStartedToast({ module: displayName, runId });
    }
  } catch (err) {
    console.warn(`[runModule] invoke failed for ${id} (backend=${backendId}):`, err);
    alert(`${t('status.error_prefix')}: ${err}`);
    if (st) st.textContent = t('status.ready');
    if (js) js.textContent = t('status.no_jobs');
    if (badge) { badge.className = 'nav-badge'; badge.textContent = ''; }
  } finally {
    if (btn) btn.disabled = false;
  }
}

export function resetForm(id) {
  Object.keys(state.files).forEach(k => {
    if (k === id || k.startsWith(`${id}-`)) state.files[k] = [];
  });
  navigate(id);
}
