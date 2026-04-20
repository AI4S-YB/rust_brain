import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderTrimmingView(container) {
  const mod = { id: 'trimming', icon: 'scissors', color: 'blue', tool: 'cutadapt-rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderTrimmingBody()}</div>`;
}

function renderTrimmingBody() {
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('trimming.input_files')}</span></div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="trimming" data-param="input_files" data-accept=".fastq,.fq,.fastq.gz,.fq.gz">
              <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
              <div class="file-drop-text">${t('trimming.drop_text')}</div>
              <div class="file-drop-hint">${t('trimming.drop_hint')}</div>
            </div>
            <div class="file-list" id="trimming-file-list"></div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('trimming.adapter_settings')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('trimming.adapter_preset')}</label>
              <select class="form-select" id="trim-preset">
                <option>${t('trimming.preset_illumina')}</option>
                <option>${t('trimming.preset_nextera')}</option>
                <option>${t('trimming.preset_smallrna')}</option>
                <option>${t('trimming.preset_bgi')}</option>
                <option>${t('trimming.preset_custom')}</option>
              </select>
            </div>
            <div class="form-group">
              <label class="form-label">${t('trimming.adapter_3')}</label>
              <input type="text" class="form-input" id="trim-adapter" data-param="adapter" value="AGATCGGAAGAGC" placeholder="AGATCGGAAGAGC">
              <span class="form-hint">${t('trimming.adapter_3_hint')}</span>
            </div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('trimming.quality_cutoff')}</label><input type="number" class="form-input" id="trim-quality" data-param="quality_cutoff" value="20" min="0" max="42"></div>
              <div class="form-group"><label class="form-label">${t('trimming.min_length')}</label><input type="number" class="form-input" id="trim-minlen" data-param="min_length" value="20" min="1"></div>
            </div>
            <div class="form-row">
              <div class="form-group"><label class="form-label">${t('trimming.max_n')}</label><input type="number" class="form-input" id="trim-maxn" value="-1"><span class="form-hint">${t('trimming.max_n_hint')}</span></div>
              <div class="form-group"><label class="form-label">${t('trimming.threads')}</label><input type="number" class="form-input" id="trim-threads" value="4" min="1" max="16"></div>
            </div>
            <div class="collapsible">
              <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('trimming.paired_options')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-paired"> ${t('trimming.paired_mode')}</label></div>
                <div class="form-group"><label class="form-label">${t('trimming.adapter_r2')}</label><input type="text" class="form-input" id="trim-adapter2" placeholder="AGATCGGAAGAGC"></div>
              </div></div>
            </div>
            <div class="collapsible">
              <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('trimming.trim_galore')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-galore"> ${t('trimming.enable_galore')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-fastqc"> ${t('trimming.post_fastqc')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-rrbs"> ${t('trimming.rrbs')}</label></div>
              </div></div>
            </div>
          </div>
          <div class="panel-footer">
            <button class="btn btn-secondary btn-sm" onclick="resetForm('trimming')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button class="btn btn-primary btn-sm" onclick="runModule('trimming')" style="background:var(--mod-blue);border-color:var(--mod-blue)"><i data-lucide="play"></i> ${t('trimming.run_trim')}</button>
          </div>
          ${renderLogPanel('trimming')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('trimming.results')}</span></div>
          <div class="panel-body">
            <div class="tabs">
              <div class="tab active" data-tab="trim-stats">${t('trimming.tab_stats')}</div>
              <div class="tab" data-tab="trim-chart">${t('trimming.tab_chart')}</div>
              <div class="tab" data-tab="trim-log">${t('qc.tab_log')}</div>
            </div>
            <div class="tab-content active" data-tab="trim-stats">
              <div class="results-summary">
                <div class="result-metric"><div class="result-metric-value">10.2M</div><div class="result-metric-label">${t('trimming.metric_reads_processed')}</div></div>
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">98.7%</div><div class="result-metric-label">${t('trimming.metric_reads_passing')}</div></div>
                <div class="result-metric"><div class="result-metric-value">4.3%</div><div class="result-metric-label">${t('trimming.metric_adapter_found')}</div></div>
                <div class="result-metric"><div class="result-metric-value">142</div><div class="result-metric-label">${t('trimming.metric_mean_length')}</div></div>
              </div>
              <table class="data-table"><thead><tr><th>${t('trimming.col_metric')}</th><th>${t('trimming.col_value')}</th></tr></thead><tbody>
                <tr><td>Total reads processed</td><td>10,243,891</td></tr>
                <tr><td>Reads with adapters</td><td>438,215 (4.3%)</td></tr>
                <tr><td>Reads too short</td><td>132,045 (1.3%)</td></tr>
                <tr><td>Reads passing filters</td><td>10,111,846 (98.7%)</td></tr>
                <tr><td>Base pairs processed</td><td>1,536,583,650</td></tr>
                <tr><td>Quality-trimmed</td><td>12,456,789 bp (0.8%)</td></tr>
                <tr><td>Total written</td><td>1,435,822,132 bp (93.4%)</td></tr>
              </tbody></table>
            </div>
            <div class="tab-content" data-tab="trim-chart">
              <div class="chart-container" id="trim-length-chart" style="height:320px;"></div>
            </div>
            <div class="tab-content" data-tab="trim-log">
              <div class="log-output"><span class="log-info">[INFO]</span> cutadapt-rs v0.1.0
<span class="log-info">[INFO]</span> Adapter: AGATCGGAAGAGC (3' regular)
<span class="log-info">[INFO]</span> Quality cutoff: 20, Min length: 20
<span class="log-info">[INFO]</span> Processing with 4 threads...
<span class="log-success">[DONE]</span> 10,243,891 reads processed in 48.2s
<span class="log-info">[INFO]</span> Output: trimmed_R1.fastq.gz</div>
            </div>
          </div>
        </div>
      </div>
    </div>`;
}
