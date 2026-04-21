import { t, getLang } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { binaryApi } from '../../api/binary.js';
import { renderAiProviderSection } from './ai-provider.js';
import { renderPluginsSection } from './plugins.js';

function settingsHeader() {
  return `
    <div class="module-header animate-slide-up">
      <div class="module-icon" style="background: rgba(92,112,128,0.08); color: var(--mod-slate);">
        <i data-lucide="settings"></i>
      </div>
      <div>
        <h1 class="module-title">${t('settings.title')}</h1>
        <p class="module-desc">${t('settings.description')}</p>
      </div>
    </div>`;
}

export async function renderSettingsView(container) {
  container.innerHTML = `<div class="module-view">${settingsHeader()}<p class="module-intro">${t('common.loading')}</p></div>`;

  let statuses = [];
  try {
    statuses = await binaryApi.getPaths();
  } catch (e) {
    container.innerHTML = `<div class="module-view">${settingsHeader()}<div class="error">Failed to load settings: ${escapeHtml(String(e))}</div></div>`;
    return;
  }

  const rows = statuses.map(s => {
    const available = s.configured_path || s.bundled_path || s.detected_on_path;
    return `
    <tr>
      <td class="tool-col">${escapeHtml(s.display_name)}</td>
      <td class="path"${s.configured_path ? ` title="${escapeHtml(s.configured_path)}"` : ''}>${s.configured_path ? escapeHtml(s.configured_path) : `<em class="text-muted">${t('settings.not_set')}</em>`}</td>
      <td class="path"${s.bundled_path ? ` title="${escapeHtml(s.bundled_path)}"` : ''}>${s.bundled_path ? escapeHtml(s.bundled_path) : `<em class="text-muted">${t('settings.not_bundled')}</em>`}</td>
      <td class="path"${s.detected_on_path ? ` title="${escapeHtml(s.detected_on_path)}"` : ''}>${s.detected_on_path ? escapeHtml(s.detected_on_path) : `<em class="text-muted">${t('settings.not_on_path')}</em>`}</td>
      <td>${available
          ? `<span class="badge badge-green">${t('settings.ok')}</span>`
          : `<span class="badge badge-coral">${t('settings.missing')}</span>`}</td>
      <td class="actions-col">
        <button class="btn btn-secondary btn-sm" data-act="browse" data-id="${escapeHtml(s.id)}">
          <i data-lucide="folder-open"></i> ${t('common.browse')}
        </button>
        ${s.configured_path
          ? `<button class="btn btn-ghost btn-sm" data-act="clear" data-id="${escapeHtml(s.id)}"><i data-lucide="x"></i> ${t('settings.clear')}</button>`
          : ''}
      </td>
    </tr>
  `;
  }).join('');
  const cur = getLang();
  const aiSection = renderAiProviderSection();
  const pluginsSection = await renderPluginsSection();
  container.innerHTML = `
    <div class="module-view">
      ${settingsHeader()}

      <div class="module-panel animate-slide-up" style="animation-delay:100ms">
        <div class="panel-header"><span class="panel-title">${t('settings.binary_section')}</span></div>
        <div class="panel-body">
          <p class="settings-intro">${t('settings.binary_intro_html')}</p>
          <div class="settings-table-wrap">
            <table class="settings-table">
              <thead><tr>
                <th>${t('settings.col_tool')}</th>
                <th>${t('settings.col_configured')}</th>
                <th>${t('settings.col_bundled')}</th>
                <th>${t('settings.col_path')}</th>
                <th>${t('settings.col_status')}</th>
                <th>${t('settings.col_actions')}</th>
              </tr></thead>
              <tbody>${rows}</tbody>
            </table>
          </div>
        </div>
      </div>

      <div class="module-panel animate-slide-up" style="animation-delay:160ms">
        <div class="panel-header"><span class="panel-title">${t('settings.language_section')}</span></div>
        <div class="panel-body">
          <div class="settings-language">
            <label class="form-checkbox"><input type="radio" name="lang-choice" value="en" ${cur === 'en' ? 'checked' : ''}> ${t('settings.language_en')}</label>
            <label class="form-checkbox"><input type="radio" name="lang-choice" value="zh" ${cur === 'zh' ? 'checked' : ''}> ${t('settings.language_zh')}</label>
          </div>
        </div>
      </div>

      ${aiSection.html}
      ${pluginsSection.html}
    </div>
  `;

  await aiSection.bind(container);
  pluginsSection.bind(container);
  if (window.lucide) window.lucide.createIcons();
}
