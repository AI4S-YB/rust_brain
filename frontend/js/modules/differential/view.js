import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderCustomPlotPanel } from '../../ui/custom-plot.js';
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
                ? `<input type="text" class="form-input" id="deseq-counts-matrix" value="${prefill.counts_matrix}" placeholder="${t('differential.counts_matrix_ph')}">`
                : `<div class="file-drop-zone" data-module="differential" data-accept=".tsv,.csv,.txt" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="table"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_counts')}</div>
                <div class="file-drop-hint">${t('differential.drop_counts_hint')}</div>
              </div>`}
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.sample_info')}</label>
              <div class="file-drop-zone" data-module="differential-coldata" data-accept=".tsv,.csv,.txt" style="padding:20px">
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
              <input type="text" class="form-input" id="deseq-design" value="condition" placeholder="${t('differential.design_var_ph')}">
              <span class="form-hint">${t('differential.design_var_hint')}</span>
            </div>
            <div class="form-group">
              <label class="form-label">${t('differential.ref_level')}</label>
              <input type="text" class="form-input" id="deseq-ref" value="control" placeholder="${t('differential.ref_level_ph')}">
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
          <div class="panel-header"><span class="panel-title">${t('differential.results')}</span></div>
          <div class="panel-body">
            <div class="tabs">
              <div class="tab active" data-tab="deseq-volcano">${t('differential.tab_volcano')}</div>
              <div class="tab" data-tab="deseq-ma">${t('differential.tab_ma')}</div>
              <div class="tab" data-tab="deseq-table">${t('differential.tab_table')}</div>
              <div class="tab" data-tab="deseq-custom">${t('differential.tab_custom')}</div>
              <div class="tab" data-tab="deseq-log">${t('qc.tab_log')}</div>
            </div>
            <div class="tab-content active" data-tab="deseq-volcano">
              <div class="results-summary">
                <div class="result-metric"><div class="result-metric-value">64,102</div><div class="result-metric-label">${t('differential.metric_total')}</div></div>
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-coral)">347</div><div class="result-metric-label">${t('differential.metric_up')}</div></div>
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-blue)">325</div><div class="result-metric-label">${t('differential.metric_down')}</div></div>
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-teal)">672</div><div class="result-metric-label">${t('differential.metric_sig')}</div></div>
              </div>
              <div class="chart-container" id="deseq-volcano-chart" style="height:380px;"></div>
            </div>
            <div class="tab-content" data-tab="deseq-ma">
              <div class="chart-container" id="deseq-ma-chart" style="height:380px;"></div>
            </div>
            <div class="tab-content" data-tab="deseq-table">
              <div style="display:flex;justify-content:flex-end;margin-bottom:8px;">
                <button class="btn btn-ghost btn-sm" onclick="exportTableAsTSV('deseq-results-table', 'deseq2_results.tsv')">${t('common.export_tsv')}</button>
              </div>
              <div style="max-height:400px;overflow-y:auto;">
                <table class="data-table" id="deseq-results-table"><thead><tr><th>${t('differential.col_gene')}</th><th>${t('differential.col_lfc')}</th><th>${t('differential.col_pvalue')}</th><th>${t('differential.col_padj')}</th></tr></thead><tbody></tbody></table>
              </div>
            </div>
            <div class="tab-content" data-tab="deseq-custom">
              ${renderCustomPlotPanel('differential')}
            </div>
            <div class="tab-content" data-tab="deseq-log">
              <div class="log-output"><span class="log-info">[INFO]</span> DESeq2_rs v0.1.0
<span class="log-info">[INFO]</span> Counts: 64,102 genes x 8 samples
<span class="log-info">[INFO]</span> Design: ~condition, Reference: control
<span class="log-info">[INFO]</span> Estimating size factors...
<span class="log-info">[INFO]</span> Estimating dispersions...
<span class="log-info">[INFO]</span> Fitting NB GLM (IRLS)...
<span class="log-info">[INFO]</span> Wald test + BH adjustment...
<span class="log-success">[DONE]</span> 672 significant genes (padj &lt; 0.01, |log2FC| &gt; 1)
<span class="log-info">[INFO]</span> Output: deseq2_results.tsv</div>
            </div>
          </div>
        </div>
      </div>
    </div>`;
}
