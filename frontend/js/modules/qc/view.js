import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';

export function renderQCView(container) {
  const mod = { id: 'qc', icon: 'microscope', color: 'teal', tool: 'fastqc-rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderQCBody()}</div>`;
}

function renderQCBody() {
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header">
            <span class="panel-title">${t('qc.input_files')}</span>
            <span class="badge badge-teal">${t('qc.files_count', { n: state.files.qc.length })}</span>
          </div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="qc" data-accept=".fastq,.fq,.fastq.gz,.fq.gz,.bam,.sam">
              <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
              <div class="file-drop-text">${t('qc.drop_text')}</div>
              <div class="file-drop-hint">${t('qc.drop_hint')}</div>
            </div>
            <div class="file-list" id="qc-file-list"></div>
          </div>
        </div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('qc.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-row">
              <div class="form-group">
                <label class="form-label">${t('qc.threads')}</label>
                <input type="number" class="form-input" id="qc-threads" value="4" min="1" max="32">
              </div>
              <div class="form-group">
                <label class="form-label">${t('qc.format')}</label>
                <select class="form-select" id="qc-format">
                  <option>${t('qc.format_auto')}</option><option>FASTQ</option><option>BAM</option><option>SAM</option>
                </select>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('qc.output_dir')}</label>
              <input type="text" class="form-input" id="qc-output" placeholder="${t('qc.output_dir_ph')}">
            </div>
            <div class="collapsible">
              <button class="collapsible-trigger" onclick="toggleCollapsible(this)">
                ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
              </button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-casava"> ${t('qc.casava')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-nogroup"> ${t('qc.nogroup')}</label></div>
                <div class="form-group"><label class="form-label">${t('qc.kmer')}</label><input type="number" class="form-input" id="qc-kmer" value="7" min="2" max="10"></div>
              </div></div>
            </div>
          </div>
          <div class="panel-footer">
            <button class="btn btn-secondary btn-sm" onclick="resetForm('qc')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button class="btn btn-primary btn-sm" onclick="runModule('qc')"><i data-lucide="play"></i> ${t('qc.run_qc')}</button>
          </div>
          ${renderLogPanel('qc')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('qc.results')}</span></div>
          <div class="panel-body">
            <div class="tabs">
              <div class="tab active" data-tab="qc-chart">${t('qc.tab_quality')}</div>
              <div class="tab" data-tab="qc-summary">${t('qc.tab_summary')}</div>
              <div class="tab" data-tab="qc-log">${t('qc.tab_log')}</div>
            </div>
            <div class="tab-content active" data-tab="qc-chart">
              <div class="chart-container" id="qc-quality-chart" style="height:320px;"></div>
            </div>
            <div class="tab-content" data-tab="qc-summary">
              <div class="results-summary">
                <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">Pass</div><div class="result-metric-label">${t('qc.metric_overall')}</div></div>
                <div class="result-metric"><div class="result-metric-value">35.2</div><div class="result-metric-label">${t('qc.metric_mean_quality')}</div></div>
                <div class="result-metric"><div class="result-metric-value">12.4M</div><div class="result-metric-label">${t('qc.metric_total_reads')}</div></div>
                <div class="result-metric"><div class="result-metric-value">150</div><div class="result-metric-label">${t('qc.metric_read_length')}</div></div>
              </div>
              <table class="data-table"><thead><tr><th>${t('qc.col_module')}</th><th>${t('qc.col_status')}</th></tr></thead><tbody>
                <tr><td>Per base sequence quality</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Per sequence quality scores</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Per base sequence content</td><td><span class="badge badge-gold">WARN</span></td></tr>
                <tr><td>Per sequence GC content</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Per base N content</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Sequence length distribution</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Sequence duplication levels</td><td><span class="badge badge-gold">WARN</span></td></tr>
                <tr><td>Overrepresented sequences</td><td><span class="badge badge-green">PASS</span></td></tr>
                <tr><td>Adapter content</td><td><span class="badge badge-green">PASS</span></td></tr>
              </tbody></table>
            </div>
            <div class="tab-content" data-tab="qc-log">
              <div class="log-output"><span class="log-info">[INFO]</span> fastqc-rs v0.12.1
<span class="log-info">[INFO]</span> Processing sample_R1.fastq.gz...
<span class="log-info">[INFO]</span> Threads: 4, Format: auto-detect
<span class="log-success">[DONE]</span> Analysis complete: 12,432,891 reads
<span class="log-info">[INFO]</span> Output written to ./fastqc_output/</div>
            </div>
          </div>
        </div>
      </div>
    </div>`;
}
