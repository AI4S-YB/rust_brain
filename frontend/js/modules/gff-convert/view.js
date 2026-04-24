import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { attachInputPicker } from '../../ui/registry-picker.js';

export function renderGffConvertView(container) {
  container.innerHTML = `
    <div class="module-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(124,92,191,0.08); color: var(--mod-purple);">
          <i data-lucide="file-cog"></i>
        </div>
        <div>
          <h1 class="module-title">${t('gff_convert.title')}</h1>
          <p class="module-desc">${t('module.powered_by')} <strong style="color: var(--mod-purple)">gffread-rs</strong></p>
          <div class="module-badges">
            <span class="badge badge-purple">${t('badge.available')}</span>
          </div>
        </div>
      </div>
      <p class="module-intro">${t('gff_convert.desc')}</p>

      <div class="module-panel animate-slide-up" style="animation-delay:100ms">
        <div class="panel-header"><span class="panel-title">${t('common.parameters')}</span></div>
        <div class="panel-body">
          <form id="form-gff-convert">
            <div class="form-group">
              <label class="form-label">${t('gff_convert.input_file')}</label>
              <div class="registry-picker"
                   data-kind="input"
                   data-input-kind="Gff"
                   data-target-name="input_file"
                   data-lineage-key="input"
                   style="margin-bottom:8px"></div>
              <div class="registry-picker"
                   data-kind="input"
                   data-input-kind="Gtf"
                   data-target-name="input_file"
                   data-lineage-key="input"
                   style="margin-bottom:8px"></div>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="input_file" data-pick="file" placeholder="/path/to/anno.gff3" required />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="input_file">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('gff_convert.target_format')}</label>
              <select class="form-select" name="target_format" required>
                <option value="gtf">${t('gff_convert.target_gtf')}</option>
                <option value="gff3">${t('gff_convert.target_gff3')}</option>
              </select>
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">
                ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
              </button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group">
                  <label class="form-label">${t('gff_convert.extra_args')}</label>
                  <textarea class="form-input form-textarea" name="extra_args" rows="3" placeholder="--keep-comments&#10;--force-exons"></textarea>
                </div>
              </div></div>
            </div>
          </form>
        </div>
        <div class="panel-footer">
          <button type="submit" form="form-gff-convert" class="btn btn-primary btn-sm" data-mod="gff-convert" data-run-button data-run-button-type="submit" data-run-label-key="gff_convert.submit" data-run-icon="play">
            <i data-lucide="play"></i> ${t('gff_convert.submit')}
          </button>
        </div>
      </div>

      <div class="module-panel animate-slide-up" style="animation-delay:160ms">
        <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
        <div class="panel-body">
          <div id="gff-convert-runs"></div>
        </div>
      </div>

      ${renderLogPanel('gff-convert')}
    </div>
  `;

  const form = container.querySelector('#form-gff-convert');
  form?.querySelectorAll('.registry-picker[data-kind="input"]').forEach(h => attachInputPicker(h));
  if (window.lucide) window.lucide.createIcons();
}
