import { MODULES } from './core/constants.js';
import { state } from './core/state.js';
import { t, navKey, applyI18n } from './core/i18n-helpers.js';
import { navigate } from './core/router.js';
import { setupEvents } from './core/events.js';
import { installRuntimeListeners } from './core/runtime.js';
import { modulesApi } from './api/modules.js';
import { installLogScrollWatch } from './ui/log-panel.js';
import { toggleCollapsible } from './ui/collapsible.js';
import { exportTableAsTSV } from './ui/export-tsv.js';
import { loadRunsForView } from './modules/run-result.js';

function collectModuleParams() {
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

async function runModule(id) {
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
  const btn = document.querySelector(`.module-view [onclick="runModule('${id}')"]`);
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

function resetForm(id) {
  Object.keys(state.files).forEach(k => {
    if (k === id || k.startsWith(`${id}-`)) state.files[k] = [];
  });
  navigate(id);
}

// === Compat: inline onclick bridges (TODO: migrate to data-action) ===
window.toggleCollapsible = toggleCollapsible;
window.exportTableAsTSV = exportTableAsTSV;
window.runModule = runModule;
window.resetForm = resetForm;

function init() {
  applyI18n(document);
  setupEvents();
  installLogScrollWatch();
  installRuntimeListeners();

  navigate(location.hash.slice(1) || 'dashboard');
  if (window.lucide) window.lucide.createIcons();
  console.log('%cRustBrain %cv0.1.0', 'font-weight:bold;font-size:14px;color:#0d7377', 'color:#57534e');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
