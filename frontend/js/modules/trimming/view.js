import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { attachInputPicker, attachSamplesPicker } from '../../ui/registry-picker.js';
import { renderFileList } from '../../ui/file-drop.js';
import { inputsApi } from '../../api/inputs.js';

export function renderTrimmingView(container) {
  const mod = { id: 'trimming', icon: 'scissors', color: 'blue', tool: 'cutadapt-rs', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderTrimmingBody()}</div>`;

  const samplesHost = container.querySelector('.registry-picker-samples');
  if (samplesHost) {
    // onPick gives input_ids — we need their actual file paths. Re-hydrate
    // from the Inputs API call that happened inside the modal.
    attachSamplesPicker(samplesHost, async ({ input_ids }) => {
      if (!input_ids.length) return;
      const all = await inputsApi.list();
      const byId = new Map((all || []).map(i => [i.id, i]));
      const files = input_ids
        .map(id => byId.get(id))
        .filter(Boolean)
        .map(i => ({ name: i.display_name, path: i.path, size: i.size_bytes || 0 }));
      state.files['trimming'] = files;
      const list = document.getElementById('trimming-file-list');
      if (list) renderFileList(list, 'trimming');
    });
  }
  const fastqHost = container.querySelector('.registry-picker[data-kind="input"][data-input-kind="Fastq"]');
  if (fastqHost) {
    attachInputPicker(fastqHost, (input) => {
      if (!input) return;
      if (!state.files.trimming) state.files.trimming = [];
      if (!state.files.trimming.some(f => f.path === input.path)) {
        state.files.trimming.push({
          name: input.display_name,
          path: input.path,
          size: input.size_bytes || 0,
          inputId: input.id,
        });
      }
      fastqHost.dataset.selectedInputIds = state.files.trimming
        .map(f => f.inputId)
        .filter(Boolean)
        .join(',');
      const list = document.getElementById('trimming-file-list');
      if (list) renderFileList(list, 'trimming');
    });
  }
  if (window.lucide) window.lucide.createIcons();
}

function renderTrimmingBody() {
  return `
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('trimming.input_files')}</span></div>
          <div class="panel-body">
            <div class="registry-picker registry-picker-samples"
                 data-kind="sample"
                 data-lineage-key="input"
                 style="margin-bottom:12px"></div>
            <div class="registry-picker"
                 data-kind="input"
                 data-input-kind="Fastq"
                 data-lineage-key="input"
                 style="margin-bottom:12px"></div>
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
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">${t('trimming.paired_options')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-paired"> ${t('trimming.paired_mode')}</label></div>
                <div class="form-group"><label class="form-label">${t('trimming.adapter_r2')}</label><input type="text" class="form-input" id="trim-adapter2" placeholder="AGATCGGAAGAGC"></div>
              </div></div>
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">${t('trimming.trim_galore')} <i data-lucide="chevron-down"></i></button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-galore"> ${t('trimming.enable_galore')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-fastqc"> ${t('trimming.post_fastqc')}</label></div>
                <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-rrbs"> ${t('trimming.rrbs')}</label></div>
              </div></div>
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="trimming"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="trimming" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="trimming.run_trim" data-run-icon="play" style="background:var(--mod-blue);border-color:var(--mod-blue)"><i data-lucide="play"></i> ${t('trimming.run_trim')}</button>
          </div>
          ${renderLogPanel('trimming')}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="trimming-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
