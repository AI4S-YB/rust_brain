import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderGeneLengthView(container) {
  const mod = { id: 'gene-length', icon: 'ruler', color: 'gold', tool: 'GTF Parser', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderGeneLengthBody()}</div>`;
  if (window.lucide) window.lucide.createIcons();
}

function renderGeneLengthBody() {
  return `
    <p class="module-intro">${t('gene_length.desc')}</p>
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('common.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('gene_length.gtf')}</label>
              <div class="file-drop-zone" data-module="gene-length-gtf" data-param="gtf" data-param-single data-accept=".gtf,.gff,.gff3">
                <div class="file-drop-icon"><i data-lucide="file-cog"></i></div>
                <div class="file-drop-text">${t('gene_length.gtf_drop')}</div>
                <div class="file-drop-hint">${t('gene_length.gtf_hint')}</div>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('gene_length.output_name')}</label>
              <input type="text" class="form-input" data-param="output_name" value="gene_lengths.tsv" />
              <span class="form-hint">${t('gene_length.output_hint')}</span>
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="gene-length"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="gene-length" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="gene_length.run" data-run-icon="play" style="background:var(--mod-gold);border-color:var(--mod-gold)"><i data-lucide="play"></i> ${t('gene_length.run')}</button>
          </div>
          ${renderLogPanel('gene-length')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="gene-length-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
