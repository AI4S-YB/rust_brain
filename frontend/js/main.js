import { MODULES } from './core/constants.js';
import { state } from './core/state.js';
import { t, navKey, applyI18n } from './core/i18n-helpers.js';
import { navigate } from './core/router.js';
import { setupEvents } from './core/events.js';
import { installRuntimeListeners } from './core/runtime.js';
import { modulesApi } from './api/modules.js';
import { installLogScrollWatch } from './ui/log-panel.js';
import { projectNew, projectOpen } from './modules/dashboard/project.js';
import { toggleCollapsible } from './ui/collapsible.js';
import { exportTableAsTSV } from './ui/export-tsv.js';
import { renderCustomPlot } from './ui/custom-plot.js';

function collectModuleParams() {
  const params = {};
  const panel = document.querySelector('.module-panel');
  if (!panel) return params;
  panel.querySelectorAll('input[id], select[id]').forEach(el => {
    if (el.type === 'checkbox') params[el.id] = el.checked;
    else params[el.id] = el.value;
  });
  return params;
}

async function runModule(id) {
  const st = document.getElementById('statusText');
  const js = document.getElementById('jobStatus');
  const mod = MODULES.find(m => m.id === id);
  const displayName = mod ? t(navKey(mod.id)) : id;
  if (st) st.textContent = `${t('status.running_prefix')} ${displayName}…`;
  if (js) js.textContent = t('status.one_job');
  const badge = document.querySelector(`.nav-item[data-view="${id}"] .nav-badge`);
  if (badge) { badge.className = 'nav-badge running'; badge.textContent = t('badge.running'); }

  const params = collectModuleParams();
  try {
    await modulesApi.validate(id, params);
    const runId = await modulesApi.run(id, params);
    if (runId) state.runIdToModule[runId] = id;
  } catch (err) {
    console.warn(`[runModule] invoke failed for ${id}:`, err);
  }

  if (st) st.textContent = t('status.ready');
  if (js) js.textContent = t('status.no_jobs');
  if (badge) { badge.className = 'nav-badge done'; badge.textContent = t('badge.done'); }
}

function resetForm(id) {
  state.files[id] = [];
  navigate(id);
}

// === Compat: inline onclick bridges (TODO: migrate to data-action) ===
window.projectNew = projectNew;
window.projectOpen = projectOpen;
window.toggleCollapsible = toggleCollapsible;
window.exportTableAsTSV = exportTableAsTSV;
window.renderCustomPlot = renderCustomPlot;
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
