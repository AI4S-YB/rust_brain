import { COLOR_MAP } from '../core/constants.js';
import { t, navKey } from '../core/i18n-helpers.js';

export function renderModuleHeader(mod) {
  const hex = COLOR_MAP[mod.color];
  const nameKey = navKey(mod.id);
  const title = mod.name || t(nameKey);
  return `
    <div class="module-header animate-slide-up">
      <div class="module-icon" style="background: ${hex}12; color: ${hex};">
        <i data-lucide="${mod.icon}"></i>
      </div>
      <div>
        <h1 class="module-title">${title}</h1>
        <p class="module-desc">${t('module.powered_by')} <strong style="color: ${hex}">${mod.tool}</strong></p>
        <div class="module-badges">
          <span class="badge badge-${mod.color}">${mod.status === 'ready' ? t('badge.available') : t('badge.coming_soon')}</span>
        </div>
      </div>
    </div>`;
}
