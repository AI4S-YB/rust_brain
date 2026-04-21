import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { pluginsApi } from '../../api/plugins.js';

export async function renderPluginsSection() {
  let diag;
  try {
    diag = await pluginsApi.listStatus();
  } catch (e) {
    return {
      html: `<div class="error">${t('settings.plugins_load_failed')}: ${escapeHtml(String(e))}</div>`,
      bind: () => {},
    };
  }
  const bundled = (diag.loaded || []).filter(p => p.source === 'bundled');
  const user = (diag.loaded || []).filter(p => p.source === 'user');
  const errors = diag.errors || [];

  const html = `
    <div class="module-panel animate-slide-up" style="animation-delay:200ms">
      <div class="panel-header"><span class="panel-title">${t('settings.plugins_section')}</span></div>
      <div class="panel-body">
        <div class="plugins-group">
          <h4>${t('settings.plugins_bundled')}</h4>
          ${renderList(bundled)}
        </div>
        <div class="plugins-group">
          <h4>${t('settings.plugins_user')}</h4>
          ${renderList(user)}
        </div>
        ${errors.length ? `
          <div class="plugins-group">
            <h4>${t('settings.plugins_errors')}</h4>
            <ul class="plugin-errors">
              ${errors.map(e => `<li><code>${escapeHtml(e.source_label)}</code>: ${escapeHtml(e.message)}</li>`).join('')}
            </ul>
          </div>` : ''}
        <div class="plugins-actions">
          <button type="button" class="btn btn-secondary btn-sm" data-act="reload-plugins">
            <i data-lucide="refresh-cw"></i> ${t('settings.plugins_reload')}
          </button>
        </div>
      </div>
    </div>
  `;
  return { html, bind: () => {} };
}

function renderList(items) {
  if (!items.length) return `<p><em>(none)</em></p>`;
  return `<ul class="plugins-list">
    ${items.map(p => `
      <li>
        <strong>${escapeHtml(p.id)}</strong>
        ${p.description ? ` — ${escapeHtml(p.description)}` : ''}
        <small> · binary: <code>${escapeHtml(p.binary_id)}</code></small>
      </li>`).join('')}
  </ul>`;
}
