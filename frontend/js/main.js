import { applyI18n } from './core/i18n-helpers.js';
import { navigate } from './core/router.js';
import { setupEvents } from './core/events.js';
import { installRuntimeListeners } from './core/runtime.js';
import { installLogScrollWatch } from './ui/log-panel.js';
import { modulesApi } from './api/modules.js';
import { setBootstrapModules } from './core/constants.js';

async function init() {
  applyI18n(document);
  setupEvents();
  installLogScrollWatch();
  installRuntimeListeners();

  try {
    const descriptors = await modulesApi.listModules();
    setBootstrapModules(descriptors);
  } catch (e) {
    console.warn('list_modules failed; falling back to static MODULES list', e);
  }

  navigate(location.hash.slice(1) || 'dashboard');
  if (window.lucide) window.lucide.createIcons();
  console.log('%cRustBrain %cv0.1.0', 'font-weight:bold;font-size:14px;color:#0d7377', 'color:#57534e');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
