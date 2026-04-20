import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderCustomPlotPanel } from '../../ui/custom-plot.js';
import { renderModuleHeader } from '../module-header.js';

export function renderNetworkView(container) {
  const mod = { id: 'network', icon: 'share-2', color: 'green', tool: 'WGCNA_rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderNetworkBody()}</div>`;
}

function renderNetworkBody() {
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('network.input_data')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('network.expr_matrix')}</label>
              <div class="file-drop-zone" data-module="network" data-accept=".csv,.tsv" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="grid-3x3"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('network.drop_expr')}</div>
                <div class="file-drop-hint">${t('network.drop_expr_hint')}</div>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('network.trait_data')}</label>
              <div class="file-drop-zone" data-module="network-trait" data-accept=".csv,.tsv" style="padding:20px">
                <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                <div class="file-drop-text" style="font-size:0.85rem">${t('network.drop_trait')}</div>
                <div class="file-drop-hint">${t('network.drop_trait_hint')}</div>
              </div>
            </div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('network.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-group"><label class="form-label">${t('network.corr_method')}</label>
              <select class="form-select" id="wgcna-corr"><option>${t('network.corr_pearson')}</option><option>${t('network.corr_biweight')}</option></select></div>
            <div class="form-group"><label class="form-label">${t('network.net_type')}</label>
              <select class="form-select" id="wgcna-nettype"><option>${t('network.net_signed')}</option><option>${t('network.net_unsigned')}</option><option>${t('network.net_signed_hybrid')}</option></select></div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('network.soft_thresh')}</label><input type="number" class="form-input" id="wgcna-thresh" value="6" min="1" max="30"><span class="form-hint">${t('network.soft_thresh_hint')}</span></div>
              <div class="form-group"><label class="form-label">${t('network.min_module')}</label><input type="number" class="form-input" id="wgcna-minmod" value="30" min="10"></div>
            </div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('network.merge_cut')}</label><input type="number" class="form-input" id="wgcna-mergecut" value="0.25" step="0.05" min="0" max="1"></div>
              <div class="form-group"><label class="form-label">${t('network.tom_type')}</label>
                <select class="form-select" id="wgcna-tom"><option>${t('network.net_signed')}</option><option>${t('network.net_unsigned')}</option></select></div>
            </div>
            <div class="collapsible">
              <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('common.advanced_options')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-pam"> ${t('network.pam')}</label></div>
                <div class="form-group"><label class="form-label">${t('network.deep_split')}</label>
                  <select class="form-select" id="wgcna-deepsplit"><option value="0">0</option><option value="1">1</option><option value="2" selected>${t('network.deep_default')}</option><option value="3">3</option><option value="4">4</option></select></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-cytoscape"> ${t('network.cytoscape')}</label></div>
              </div></div>
            </div>
          </div>
          <div class="panel-footer">
            <button class="btn btn-secondary btn-sm"><i data-lucide="zap"></i> ${t('network.pick_threshold')}</button>
            <button class="btn btn-primary btn-sm" onclick="runModule('network')" style="background:var(--mod-green);border-color:var(--mod-green)"><i data-lucide="play"></i> ${t('network.run_wgcna')}</button>
          </div>
          ${renderLogPanel('network')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('network.results')}</span></div>
          <div class="panel-body">
            <div class="tabs">
              <div class="tab active" data-tab="wgcna-modules">${t('network.tab_modules')}</div>
              <div class="tab" data-tab="wgcna-trait">${t('network.tab_trait')}</div>
              <div class="tab" data-tab="wgcna-custom">${t('network.tab_custom')}</div>
              <div class="tab" data-tab="wgcna-log">${t('qc.tab_log')}</div>
            </div>
            <div class="tab-content active" data-tab="wgcna-modules">
              <div class="results-summary">
                <div class="result-metric"><div class="result-metric-value">5,000</div><div class="result-metric-label">${t('network.metric_genes')}</div></div>
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">12</div><div class="result-metric-label">${t('network.metric_modules')}</div></div>
                <div class="result-metric"><div class="result-metric-value">R²=0.87</div><div class="result-metric-label">${t('network.metric_fit')}</div></div>
              </div>
              <div class="chart-container" id="wgcna-module-chart" style="height:320px;"></div>
            </div>
            <div class="tab-content" data-tab="wgcna-trait">
              <div class="chart-container" id="wgcna-trait-chart" style="height:380px;"></div>
            </div>
            <div class="tab-content" data-tab="wgcna-custom">
              ${renderCustomPlotPanel('network')}
            </div>
            <div class="tab-content" data-tab="wgcna-log">
              <div class="log-output"><span class="log-info">[INFO]</span> WGCNA_rs v0.1.0
<span class="log-info">[INFO]</span> Matrix: 50 samples x 5,000 genes
<span class="log-info">[INFO]</span> Correlation: Pearson, Network: Signed
<span class="log-info">[INFO]</span> Soft threshold = 6 (R² = 0.87)
<span class="log-info">[INFO]</span> Computing TOM...
<span class="log-info">[INFO]</span> Hierarchical clustering (NN-chain)...
<span class="log-info">[INFO]</span> Dynamic tree cutting...
<span class="log-info">[INFO]</span> Merging modules (cutHeight=0.25)...
<span class="log-success">[DONE]</span> 12 modules identified
<span class="log-info">[INFO]</span> Output: module_assignments.tsv</div>
            </div>
          </div>
        </div>
      </div>
    </div>`;
}
