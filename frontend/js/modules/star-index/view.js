import { state } from '../../core/state.js';
import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { renderLogPanel } from '../../ui/log-panel.js';

export function renderStarIndexView(container) {
  const prefill = (state.prefill && state.prefill.star_index) || {};
  state.prefill = {};
  const gtfValue = prefill.gtf_file || '';
  container.innerHTML = `
    <div class="module-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(124,92,191,0.08); color: var(--mod-purple);">
          <i data-lucide="database"></i>
        </div>
        <div>
          <h1 class="module-title">${t('star_index.title')}</h1>
          <p class="module-desc">${t('module.powered_by')} <strong style="color: var(--mod-purple)">STAR_rs</strong></p>
          <div class="module-badges">
            <span class="badge badge-purple">${t('badge.available')}</span>
          </div>
        </div>
      </div>
      <p class="module-intro">${t('star_index.desc')}</p>

      <div class="module-panel animate-slide-up" style="animation-delay:100ms">
        <div class="panel-header"><span class="panel-title">${t('common.parameters')}</span></div>
        <div class="panel-body">
          <form id="form-star-index">
            <div class="form-group">
              <label class="form-label">${t('star_index.genome_fasta')}</label>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="genome_fasta" data-pick="file" placeholder="/path/to/genome.fa" required />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="genome_fasta">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">${t('star_index.gtf')}</label>
              <div class="input-with-browse">
                <input type="text" class="form-input" name="gtf_file" data-pick="file" value="${escapeHtml(gtfValue)}" placeholder="/path/to/annotation.gtf" required />
                <button type="button" class="btn btn-secondary btn-sm" data-pick-for="gtf_file">
                  <i data-lucide="folder-open"></i> ${t('common.browse')}
                </button>
              </div>
              <span class="form-hint">
                ${t('star_index.gtf_hint')}
                <a href="#gff-convert" class="form-hint-link">${t('star_index.gtf_hint_link')}</a>
              </span>
            </div>
            <div class="form-row three-col">
              <div class="form-group">
                <label class="form-label">${t('star_index.threads')}</label>
                <input type="number" class="form-input" name="threads" value="4" min="1" />
              </div>
              <div class="form-group">
                <label class="form-label">${t('star_index.sjdb')}</label>
                <input type="number" class="form-input" name="sjdb_overhang" value="100" min="1" />
              </div>
              <div class="form-group">
                <label class="form-label">${t('star_index.sa_nbases')}</label>
                <input type="number" class="form-input" name="genome_sa_index_nbases" value="14" min="1" max="18" />
              </div>
            </div>
            <div class="collapsible">
              <button type="button" class="collapsible-trigger" data-act="collapsible-toggle">
                ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
              </button>
              <div class="collapsible-content"><div class="collapsible-body">
                <div class="form-group">
                  <label class="form-label">${t('star_index.extra_args')}</label>
                  <textarea class="form-input form-textarea" name="extra_args" rows="3" placeholder="--limitGenomeGenerateRAM&#10;31000000000"></textarea>
                </div>
              </div></div>
            </div>
          </form>
        </div>
        <div class="panel-footer">
          <button type="submit" form="form-star-index" class="btn btn-primary btn-sm">
            <i data-lucide="play"></i> ${t('star_index.submit')}
          </button>
        </div>
      </div>

      <div class="module-panel animate-slide-up" style="animation-delay:160ms">
        <div class="panel-header"><span class="panel-title">${t('common.runs')}</span></div>
        <div class="panel-body">
          <div id="star-index-runs"></div>
        </div>
      </div>

      ${renderLogPanel('star-index')}
    </div>
  `;
}
