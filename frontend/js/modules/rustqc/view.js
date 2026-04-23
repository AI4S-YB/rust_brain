import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderRustqcView(container) {
  const mod = { id: 'rustqc', icon: 'shield-check', color: 'teal', tool: 'RustQC', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderRustqcBody()}</div>`;
}

function renderRustqcBody() {
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('rustqc.input_bams')}</span></div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="rustqc" data-param="input_bams" data-accept=".bam,.sam,.cram">
              <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
              <div class="file-drop-text">${t('rustqc.drop_text')}</div>
              <div class="file-drop-hint">${t('rustqc.drop_hint')}</div>
            </div>
            <div class="file-list" id="rustqc-file-list"></div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:140ms">
          <div class="panel-header"><span class="panel-title">${t('rustqc.annotation')}</span></div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="rustqc" data-param="gtf" data-param-single data-accept=".gtf,.gtf.gz">
              <div class="file-drop-icon"><i data-lucide="file-text"></i></div>
              <div class="file-drop-text">${t('rustqc.drop_gtf')}</div>
              <div class="file-drop-hint">${t('rustqc.drop_gtf_hint')}</div>
            </div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:180ms">
          <div class="panel-header"><span class="panel-title">${t('rustqc.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-row">
              <div class="form-group">
                <label class="form-checkbox"><input type="checkbox" data-param="paired"> ${t('rustqc.paired')}</label>
                <span class="form-hint">${t('rustqc.paired_hint')}</span>
              </div>
            </div>
            <div class="form-row">
              <div class="form-group">
                <label class="form-label">${t('rustqc.stranded')}</label>
                <select class="form-select" data-param="stranded">
                  <option value="">${t('rustqc.stranded_auto')}</option>
                  <option value="unstranded">unstranded</option>
                  <option value="forward">forward</option>
                  <option value="reverse">reverse</option>
                </select>
              </div>
              <div class="form-group">
                <label class="form-label">${t('rustqc.threads')}</label>
                <input type="number" class="form-input" data-param="threads" value="4" min="1">
              </div>
              <div class="form-group">
                <label class="form-label">${t('rustqc.mapq')}</label>
                <input type="number" class="form-input" data-param="mapq" value="30" min="0">
              </div>
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">${t('rustqc.advanced')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group">
                  <label class="form-label">${t('rustqc.output_dir')}</label>
                  <input type="text" class="form-input" data-param="output_dir" placeholder="${t('rustqc.output_dir_ph')}">
                </div>
                <div class="form-group">
                  <span class="form-hint">${t('rustqc.advanced_hint')}</span>
                </div>
              </div></div>
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="rustqc"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="rustqc" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="rustqc.run" data-run-icon="play" style="background:var(--mod-teal);border-color:var(--mod-teal)"><i data-lucide="play"></i> ${t('rustqc.run')}</button>
          </div>
          ${renderLogPanel('rustqc')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="rustqc-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
