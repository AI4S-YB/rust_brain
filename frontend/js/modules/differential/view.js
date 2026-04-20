import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderDifferentialView(container) {
  const mod = { id: 'differential', icon: 'flame', color: 'coral', tool: 'DESeq2_rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderDifferentialBody()}</div>`;
}

function renderDifferentialBody() {
  const prefill = (state.prefill && state.prefill.differential) || {};
  state.prefill = {};
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('differential.input_data')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('differential.counts_matrix')}</label>
              ${prefill.counts_matrix
                ? `<input type="text" class="form-input" id="deseq-counts-matrix" data-param="counts_path" value="${prefill.counts_matrix}" placeholder="${t('differential.counts_matrix_ph')}">`
                : `<div class="file-drop-zone" data-module="differential" data-param="counts_path" data-param-single data-accept=".tsv,.csv,.txt" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="table"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_counts')}</div>
                <div class="file-drop-hint">${t('differential.drop_counts_hint')}</div>
              </div>`}
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.sample_info')}</label>
              <div class="file-drop-zone" data-module="differential-coldata" data-param="coldata_path" data-param-single data-accept=".tsv,.csv,.txt" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_coldata')}</div>
                <div class="file-drop-hint">${t('differential.drop_coldata_hint')}</div>
              </div>
            </div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('differential.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('differential.design_var')}</label>
              <input type="text" class="form-input" id="deseq-design" data-param="design" value="~condition" placeholder="${t('differential.design_var_ph')}">
              <span class="form-hint">${t('differential.design_var_hint')}</span>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.ref_level')}</label>
              <input type="text" class="form-input" id="deseq-ref" data-param="reference" value="control" placeholder="${t('differential.ref_level_ph')}">
              <span class="form-hint">${t('differential.ref_level_hint')}</span>
            </div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('differential.padj')}</label><input type="number" class="form-input" id="deseq-padj" value="0.01" step="0.01" min="0" max="1"></div>
              <div class="form-group"><label class="form-label">${t('differential.lfc')}</label><input type="number" class="form-input" id="deseq-lfc" value="1.0" step="0.1" min="0"></div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.output_file')}</label>
              <input type="text" class="form-input" id="deseq-output" value="deseq2_results.tsv" placeholder="results.tsv">
            </div>
          </div>
          <div class="panel-footer">
            <button class="btn btn-secondary btn-sm" onclick="resetForm('differential')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button class="btn btn-primary btn-sm" onclick="runModule('differential')" style="background:var(--mod-coral);border-color:var(--mod-coral)"><i data-lucide="play"></i> ${t('differential.run_deseq')}</button>
          </div>
          ${renderLogPanel('differential')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="differential-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
