import { t, navKey } from '../core/i18n-helpers.js';

export function renderComingSoon(mod) {
  const nameKey = navKey(mod.id);
  return `
    <div class="card animate-slide-up" style="animation-delay:100ms">
      <div class="empty-state" style="padding:64px 24px">
        <div class="empty-state-icon"><i data-lucide="${mod.icon}"></i></div>
        <h3 class="empty-state-title">${t(nameKey)}</h3>
        <p class="empty-state-text">${t('module.soon_body', { tool: `<strong>${mod.tool}</strong>` })}</p>
        <div style="margin-top:20px"><span class="badge badge-muted" style="font-size:0.8rem;padding:6px 14px">${t('badge.in_development')}</span></div>
      </div>
    </div>`;
}

export function renderEmptyState(msg) {
  return `<div class="empty-state"><div class="empty-state-icon"><i data-lucide="alert-circle"></i></div><h3 class="empty-state-title">${msg}</h3></div>`;
}
