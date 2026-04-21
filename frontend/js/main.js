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
    injectPluginSidebarEntries(descriptors);
  } catch (e) {
    console.warn('list_modules failed; falling back to static MODULES list', e);
  }

  navigate(location.hash.slice(1) || 'dashboard');
  if (window.lucide) window.lucide.createIcons();
  console.log('%cRustBrain %cv0.1.0', 'font-weight:bold;font-size:14px;color:#0d7377', 'color:#57534e');
}

function injectPluginSidebarEntries(descriptors) {
  const plugins = descriptors.filter(d => d.source !== 'builtin');
  if (plugins.length === 0) return;
  const sidebarNav = document.querySelector('.sidebar-nav');
  if (!sidebarNav) return;

  // Insert before the System section if present, else append.
  const systemSection = Array.from(sidebarNav.querySelectorAll('.nav-section'))
    .find(s => s.querySelector('[data-view="settings"]'));

  const section = document.createElement('div');
  section.className = 'nav-section';
  section.innerHTML = `
    <div class="nav-section-title">Plugins</div>
    ${plugins.map(p => `
      <a class="nav-item" data-view="${p.view_id}" data-color="plug" href="#${p.view_id}">
        <i data-lucide="${p.icon || 'plug'}"></i>
        <span>${escapePluginName(p.name)}</span>
        <span class="nav-plug-badge" title="Third-party plugin"><i data-lucide="plug"></i></span>
      </a>
    `).join('')}
  `;

  if (systemSection) {
    sidebarNav.insertBefore(section, systemSection);
  } else {
    sidebarNav.appendChild(section);
  }
  if (window.lucide) window.lucide.createIcons();
}

function escapePluginName(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
