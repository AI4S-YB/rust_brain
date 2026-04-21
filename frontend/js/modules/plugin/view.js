import { state } from '../../core/state.js';
import { getLang } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { modulesApi } from '../../api/modules.js';
import { escapeHtml } from '../../ui/escape.js';
import { binaryApi } from '../../api/binary.js';
import { renderMissingBinaryCard } from './missing-binary.js';

export async function renderPluginView(container, viewId) {
  container.innerHTML = `<div class="module-view"><p>Loading…</p></div>`;
  let manifest;
  try {
    manifest = await modulesApi.getPluginManifest(viewId);
  } catch (e) {
    container.innerHTML = `<div class="module-view"><div class="error">Failed to load plugin manifest: ${escapeHtml(String(e))}</div></div>`;
    return;
  }
  const lang = getLang();
  const mod = {
    id: viewId,
    name: localized(manifest.strings, 'name', lang, manifest.name),
    icon: manifest.icon || 'plug',
    color: 'plug',
    tool: manifest.binary_id,
    status: 'ready',
  };
  const header = renderModuleHeader(mod);

  // Check if the plugin's binary is configured / discoverable.
  let binaryOk = true;
  try {
    const binaries = await binaryApi.getPaths();
    const my = binaries.find(b => b.id === manifest.binary_id);
    binaryOk = !!(my && (my.configured_path || my.bundled_path || my.detected_on_path));
  } catch (_) {
    // If binary status fails, fall through and let the user see the form;
    // the run will surface the resolution error in the log.
    binaryOk = true;
  }

  if (!binaryOk) {
    container.innerHTML = `<div class="module-view">${header}${renderMissingBinaryCard(manifest)}</div>`;
    if (window.lucide) window.lucide.createIcons();
    return;
  }

  const body = renderPluginBody(manifest, lang, viewId);
  container.innerHTML = `<div class="module-view">${header}${body}</div>`;
}

function localized(strings, key, lang, fallback) {
  if (!strings) return fallback;
  return strings[`${key}_${lang}`] || strings[`${key}_en`] || fallback;
}

function renderPluginBody(m, lang, viewId) {
  const desc = localized(m.strings, 'description', lang, m.description || '');
  const params = m.params || [];
  return `
    <div class="module-layout">
      <div>
        ${desc ? `<p class="module-intro">${escapeHtml(desc)}</p>` : ''}
        <div class="module-panel animate-slide-up">
          <div class="panel-header"><span class="panel-title">Parameters</span></div>
          <div class="panel-body">
            ${params.map(p => renderParam(p, lang, viewId)).join('')}
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="${viewId}"><i data-lucide="rotate-ccw"></i> Reset</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="${viewId}" data-run-button data-run-button-act="run-module" data-run-button-type="button"><i data-lucide="play"></i> Run</button>
          </div>
          ${renderLogPanel(viewId)}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">Runs</span></div>
          <div class="panel-body"><div id="${viewId}-runs"></div></div>
        </div>
      </div>
    </div>
  `;
}

function renderParam(p, lang, viewId) {
  const label = localized(p, 'label', lang, p.name);
  const help = localized(p, 'help', lang, '');
  const ui = p.ui || defaultUiForType(p.type);
  const dataParam = `data-param="${escapeHtml(p.name)}"`;

  if (p.type === 'file_list' || (p.type === 'file' && ui === 'drop_zone')) {
    const single = p.type === 'file' ? 'data-param-single' : '';
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}${p.required ? ' *' : ''}</label>
        <div class="file-drop-zone" data-module="${viewId}" ${dataParam} ${single}>
          <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
          <div class="file-drop-text">Drop files here or click to browse</div>
        </div>
        <div class="file-list" id="${viewId}-${p.name}-list"></div>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'boolean') {
    return `
      <div class="form-group">
        <label class="form-checkbox">
          <input type="checkbox" ${dataParam} ${p.default ? 'checked' : ''}>
          ${escapeHtml(label)}
        </label>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'enum') {
    const opts = (p.values || []).map(v => `<option value="${escapeHtml(v)}" ${p.default === v ? 'selected' : ''}>${escapeHtml(v)}</option>`).join('');
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}</label>
        <select class="form-select" ${dataParam}>${opts}</select>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'integer') {
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}</label>
        <input type="number" class="form-input" ${dataParam} value="${p.default ?? ''}" ${p.minimum != null ? `min="${p.minimum}"` : ''} ${p.maximum != null ? `max="${p.maximum}"` : ''}>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  // string / output_dir / directory / file (path input)
  return `
    <div class="form-group">
      <label class="form-label">${escapeHtml(label)}${p.required ? ' *' : ''}</label>
      <input type="text" class="form-input" ${dataParam} value="${escapeHtml(p.default ?? '')}" placeholder="${escapeHtml(label)}">
      ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
    </div>`;
}

function defaultUiForType(type) {
  switch (type) {
    case 'file_list': return 'drop_zone';
    case 'boolean': return 'checkbox';
    case 'enum': return 'select';
    case 'integer': return 'number';
    default: return 'text';
  }
}
