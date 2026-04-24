import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { renderFileList } from '../../ui/file-drop.js';

export function renderCountsMergeView(container) {
  const mod = { id: 'counts-merge', icon: 'table', color: 'green', tool: 'STAR ReadsPerGene', status: 'ready' };
  container.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderCountsMergeBody()}</div>`;
  const list = container.querySelector('#counts-merge-file-list');
  if (list) renderFileList(list, 'counts-merge');
  if (window.lucide) window.lucide.createIcons();
}

function renderCountsMergeBody() {
  if (!state.files['counts-merge']) state.files['counts-merge'] = [];
  return `
    <p class="module-intro">${t('counts_merge.desc')}</p>
    <div class="module-layout">
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:100ms">
          <div class="panel-header"><span class="panel-title">${t('counts_merge.input_files')}</span></div>
          <div class="panel-body">
            <div class="file-drop-zone" data-module="counts-merge" data-param="reads_per_gene" data-accept=".tab,.tsv,.txt">
              <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
              <div class="file-drop-text">${t('counts_merge.drop_text')}</div>
              <div class="file-drop-hint">${t('counts_merge.drop_hint')}</div>
            </div>
            <div class="file-list" id="counts-merge-file-list"></div>
          </div>
        </div>

        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">${t('counts_merge.parameters')}</span></div>
          <div class="panel-body">
            <div class="form-group">
              <label class="form-label">${t('counts_merge.sample_names')}</label>
              <textarea class="form-input form-textarea" data-param="sample_names" rows="4" placeholder="S1&#10;S2"></textarea>
              <span class="form-hint">${t('counts_merge.sample_names_hint')}</span>
            </div>
            <div class="form-row">
              <div class="form-group">
                <label class="form-label">${t('counts_merge.strand')}</label>
                <select class="form-select" data-param="strand">
                  <option value="unstranded">${t('counts_merge.strand_unstranded')}</option>
                  <option value="forward">${t('counts_merge.strand_forward')}</option>
                  <option value="reverse">${t('counts_merge.strand_reverse')}</option>
                </select>
              </div>
              <div class="form-group">
                <label class="form-label">${t('counts_merge.output_name')}</label>
                <input type="text" class="form-input" data-param="output_name" value="counts_matrix.tsv" />
              </div>
            </div>
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="counts-merge"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="counts-merge" data-run-button data-run-button-act="run-module" data-run-button-type="button" data-run-label-key="counts_merge.run" data-run-icon="play" style="background:var(--mod-green);border-color:var(--mod-green)"><i data-lucide="play"></i> ${t('counts_merge.run')}</button>
          </div>
          ${renderLogPanel('counts-merge')}
        </div>
      </div>

      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:220ms">
          <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
          <div class="panel-body">
            <div id="counts-merge-runs"></div>
          </div>
        </div>
      </div>
    </div>`;
}
