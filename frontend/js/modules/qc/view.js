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
            <span class="badge badge-teal" data-files-count="qc" data-files-count-key="qc.files_count">${t('qc.files_count', { n: state.files.qc.length })}</span>
          </div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="qc" data-param="input_files" data-accept=".fastq,.fq,.fastq.gz,.fq.gz,.bam,.sam">
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
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">
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
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="qc"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="qc" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="qc.run_qc" data-run-icon="play"><i data-lucide="play"></i> ${t('qc.run_qc')}</button>
          </div>
          ${renderLogPanel('qc')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="qc-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
