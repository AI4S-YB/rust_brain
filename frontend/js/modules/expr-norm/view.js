import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderExprNormView(container) {
  const mod = { id: 'expr-norm', icon: 'sigma', color: 'green', tool: 'TPM / FPKM', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderExprNormBody()}</div>`;
  if (window.lucide) window.lucide.createIcons();
}

function renderExprNormBody() {
  return `
    <p class="module-intro">${t('expr_norm.desc')}</p>
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('expr_norm.input_files')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('expr_norm.counts')}</label>
              <div class="file-drop-zone" data-module="expr-norm-counts" data-param="counts" data-param-single data-accept=".tsv,.txt,.csv">
                <div class="file-drop-icon"><i data-lucide="table"></i></div>
                <div class="file-drop-text">${t('expr_norm.counts_drop')}</div>
                <div class="file-drop-hint">${t('expr_norm.counts_hint')}</div>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('expr_norm.lengths')}</label>
              <div class="file-drop-zone" data-module="expr-norm-lengths" data-param="lengths" data-param-single data-accept=".tsv,.txt">
                <div class="file-drop-icon"><i data-lucide="ruler"></i></div>
                <div class="file-drop-text">${t('expr_norm.lengths_drop')}</div>
                <div class="file-drop-hint">${t('expr_norm.lengths_hint')}</div>
              </div>
            </div>
          </div>
        </div>

        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('common.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-row">
              <div class="form-group">
                <label class="form-label">${t('expr_norm.length_mode')}</label>
                <select class="form-select" data-param="length_mode">
                  <option value="union">${t('expr_norm.length_union')}</option>
                  <option value="longest">${t('expr_norm.length_longest')}</option>
                </select>
                <span class="form-hint">${t('expr_norm.length_mode_hint')}</span>
              </div>
              <div class="form-group">
                <label class="form-label">${t('expr_norm.method')}</label>
                <select class="form-select" data-param="method">
                  <option value="tpm">TPM</option>
                  <option value="fpkm">FPKM</option>
                  <option value="both">${t('expr_norm.method_both')}</option>
                </select>
                <span class="form-hint">${t('expr_norm.method_hint')}</span>
              </div>
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="expr-norm"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="expr-norm" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="expr_norm.run" data-run-icon="play" style="background:var(--mod-green);border-color:var(--mod-green)"><i data-lucide="play"></i> ${t('expr_norm.run')}</button>
          </div>
          ${renderLogPanel('expr-norm')}
        </div>
      </div>

      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="expr-norm-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
