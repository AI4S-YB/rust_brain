import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { attachSamplesPicker } from '../../ui/registry-picker.js';
import { renderFileList } from '../../ui/file-drop.js';
import { inputsApi } from '../../api/inputs.js';

export function renderQCView(container) {
  const mod = { id: 'qc', icon: 'microscope', color: 'teal', tool: 'fastqc-rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderQCBody()}</div>`;

  const samplesHost = container.querySelector('.registry-picker-samples');
  if (samplesHost) {
    attachSamplesPicker(samplesHost, async ({ input_ids }) => {
      if (!input_ids.length) return;
      const all = await inputsApi.list();
      const byId = new Map((all || []).map(i => [i.id, i]));
      state.files['qc'] = input_ids
        .map(id => byId.get(id))
        .filter(Boolean)
        .map(i => ({ name: i.display_name, path: i.path, size: i.size_bytes || 0 }));
      const list = document.getElementById('qc-file-list');
      if (list) renderFileList(list, 'qc');
      document.querySelectorAll('[data-files-count="qc"]').forEach(el => {
        el.textContent = t('qc.files_count', { n: state.files.qc.length });
      });
    });
  }
  if (window.lucide) window.lucide.createIcons();
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
            <div class="registry-picker registry-picker-samples"
                 data-kind="sample"
                 data-lineage-key="input"
                 style="margin-bottom:12px"></div>
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
                <input type="number" class="form-input" id="qc-threads" data-param="threads" value="4" min="1" max="32">
              </div>
              <div class="form-group">
                <label class="form-label">${t('qc.format')}</label>
                <select class="form-select" id="qc-format" data-param="sequence_format">
                  <option value="">${t('qc.format_auto')}</option><option value="fastq">FASTQ</option><option value="bam">BAM</option><option value="sam">SAM</option>
                </select>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('qc.output_dir')}</label>
              <input type="text" class="form-input" id="qc-output" data-param="output_dir" placeholder="${t('qc.output_dir_ph')}">
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">
                ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
              </button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-casava" data-param="casava"> ${t('qc.casava')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-nogroup" data-param="nogroup"> ${t('qc.nogroup')}</label></div>
                <div class="form-group"><label class="form-label">${t('qc.kmer')}</label><input type="number" class="form-input" id="qc-kmer" data-param="kmer_size" value="7" min="2" max="10"></div>
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
