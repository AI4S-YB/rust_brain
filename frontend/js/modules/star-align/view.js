import { t } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { attachAssetPicker, attachSamplesPicker } from '../../ui/registry-picker.js';

export function renderStarAlignView(container) {
  container.innerHTML = `
    <div class="module-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(124,92,191,0.08); color: var(--mod-purple);">
          <i data-lucide="git-merge"></i>
        </div>
        <div>
          <h1 class="module-title">${t('star_align.title')}</h1>
          <p class="module-desc">${t('module.powered_by')} <strong style="color: var(--mod-purple)">STAR_rs</strong></p>
          <div class="module-badges">
            <span class="badge badge-purple">${t('badge.available')}</span>
          </div>
        </div>
      </div>
      <p class="module-intro">${t('star_align.desc')}</p>

      <div class="module-panel animate-slide-up" style="animation-delay:100ms">
        <div class="panel-header"><span class="panel-title">${t('common.parameters')}</span></div>
        <div class="panel-body">
          <form id="form-star-align">
            <div class="form-group">
              <div class="registry-picker"
                   data-kind="asset"
                   data-asset-kind="StarIndex"
                   data-target-name="genome_dir"
                   data-lineage-key="asset"></div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('star_align.genome_dir')}</label>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="genome_dir" required placeholder="/path/to/star_index" />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="genome_dir" data-pick-mode="dir">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
            </div>
            <div class="form-group">
              <div class="registry-picker registry-picker-samples"
                   data-kind="sample"
                   data-lineage-key="input"></div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('star_align.reads_1')}</label>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="reads_1" required placeholder="/path/to/S1_R1.fq.gz /path/to/S2_R1.fq.gz" />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="reads_1" data-pick-mode="multi">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('star_align.reads_2')}</label>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="reads_2" placeholder="/path/to/S1_R2.fq.gz /path/to/S2_R2.fq.gz" />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="reads_2" data-pick-mode="multi">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('star_align.sample_names')}</label>
              <textarea class="form-input form-textarea" name="sample_names" rows="3" placeholder="S1&#10;S2"></textarea>
            </div>
            <div class="form-row">
              <div class="form-group">
                <label class="form-label">${t('star_align.threads')}</label>
                <input type="number" class="form-input" name="threads" value="4" min="1" />
              </div>
              <div class="form-group">
                <label class="form-label">${t('star_align.strand')}</label>
                <div class="segmented">
                  <label><input type="radio" name="strand" value="unstranded" checked /> <span>${t('star_align.strand_unstranded')}</span></label>
                  <label><input type="radio" name="strand" value="forward" /> <span>${t('star_align.strand_forward')}</span></label>
                  <label><input type="radio" name="strand" value="reverse" /> <span>${t('star_align.strand_reverse')}</span></label>
                </div>
              </div>
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">
                ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
              </button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group">
                  <label class="form-label">${t('star_align.extra_args')}</label>
                  <textarea class="form-input form-textarea" name="extra_args" rows="3" placeholder="--outFilterMultimapNmax&#10;10"></textarea>
                </div>
              </div></div>
            </div>
          </form>
        </div>
        <div class="panel-footer">
          <button type="submit" form="form-star-align" class="btn btn-primary btn-sm" data-mod="star-align" data-run-button data-run-button-type="submit" data-run-label-key="star_align.submit" data-run-icon="play">
            <i data-lucide="play"></i> ${t('star_align.submit')}
          </button>
        </div>
      </div>

      <div class="module-panel animate-slide-up" style="animation-delay:160ms">
        <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
        <div class="panel-body">
          <div id="star-align-runs"></div>
        </div>
      </div>

      ${renderLogPanel('star-align')}
    </div>
  `;

  const form = container.querySelector('#form-star-align');
  const assetHost = form?.querySelector('.registry-picker[data-kind="asset"]');
  if (assetHost) attachAssetPicker(assetHost);

  const samplesHost = form?.querySelector('.registry-picker-samples');
  if (samplesHost) {
    attachSamplesPicker(samplesHost, ({ r1_paths, r2_paths, names }) => {
      const r1 = form.querySelector('input[name="reads_1"]');
      const r2 = form.querySelector('input[name="reads_2"]');
      const nm = form.querySelector('textarea[name="sample_names"]');
      if (r1) r1.value = r1_paths.join(' ');
      if (r2) r2.value = r2_paths.join(' ');
      if (nm) nm.value = names.join('\n');
    });
  }
  if (window.lucide) window.lucide.createIcons();
}
