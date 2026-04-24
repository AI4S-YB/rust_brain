import { t } from '../../core/i18n-helpers.js';

const _invoke = typeof window !== 'undefined' ? window.__TAURI__?.core?.invoke : undefined;
const api = _invoke
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

export function renderBamToolsView(content) {
  content.innerHTML = `
    <div class="module-view bam-tools">
      <header class="module-header">
        <div class="module-title">
          <div class="module-title-icon" style="--icon-color: var(--mod-coral, #c9503c)"><i data-lucide="database"></i></div>
          <div>
            <h1>${t('nav.bam_tools')}</h1>
            <p class="module-subtitle">${t('utility.bam_tools.subtitle')}</p>
          </div>
        </div>
      </header>

      <section class="card bam-tools-section">
        <h2>${t('utility.bam_tools.input_title')}</h2>
        <div class="bam-tools-row">
          <button class="btn btn-primary" data-act="bam-open"><i data-lucide="folder-open"></i> ${t('utility.bam_tools.open_bam')}</button>
          <span class="bam-tools-path" data-bam-path>${t('utility.bam_tools.no_file')}</span>
        </div>
        <div class="bam-tools-index-status" data-bam-index-status></div>
      </section>

      <section class="card bam-tools-section" data-bam-index-card hidden>
        <h2>${t('utility.bam_tools.index_title')}</h2>
        <p class="module-help">${t('utility.bam_tools.index_help')}</p>
        <div class="bam-tools-row">
          <button class="btn btn-primary" data-act="bam-index"><i data-lucide="list-tree"></i> ${t('utility.bam_tools.build_index')}</button>
        </div>
      </section>

      <section class="card bam-tools-section" data-bam-extract-card hidden>
        <h2>${t('utility.bam_tools.extract_title')}</h2>
        <p class="module-help">${t('utility.bam_tools.extract_help')}</p>
        <div class="bam-tools-grid">
          <label>
            <span>${t('utility.bam_tools.region')}</span>
            <input type="text" class="form-input" data-bam-region placeholder="chr1:10000-20000">
          </label>
          <label>
            <span>${t('utility.bam_tools.chromosome_hint')}</span>
            <select class="form-input" data-bam-chrom>
              <option value="">—</option>
            </select>
          </label>
        </div>
        <div class="bam-tools-grid">
          <label class="bam-tools-output">
            <span>${t('utility.bam_tools.output_path')}</span>
            <input type="text" class="form-input" data-bam-output placeholder="/path/to/region.bam">
          </label>
          <div class="bam-tools-output-actions">
            <button class="btn btn-secondary" data-act="bam-output-browse"><i data-lucide="folder-open"></i> ${t('utility.bam_tools.choose_output_dir')}</button>
          </div>
        </div>
        <div class="bam-tools-row">
          <button class="btn btn-primary" data-act="bam-extract"><i data-lucide="scissors"></i> ${t('utility.bam_tools.extract')}</button>
        </div>
      </section>

      <section class="card bam-tools-section" data-bam-log-card hidden>
        <h2>${t('utility.bam_tools.log_title')}</h2>
        <pre class="bam-tools-log" data-bam-log></pre>
      </section>
    </div>
  `;

  const pathEl = content.querySelector('[data-bam-path]');
  const statusEl = content.querySelector('[data-bam-index-status]');
  const indexCard = content.querySelector('[data-bam-index-card]');
  const extractCard = content.querySelector('[data-bam-extract-card]');
  const logCard = content.querySelector('[data-bam-log-card]');
  const logEl = content.querySelector('[data-bam-log]');
  const chromSelect = content.querySelector('[data-bam-chrom]');
  const regionInput = content.querySelector('[data-bam-region]');
  const outputInput = content.querySelector('[data-bam-output]');

  const s = { bamPath: null, indexed: false, refs: [] };

  function log(msg) {
    logCard.hidden = false;
    const ts = new Date().toLocaleTimeString();
    logEl.textContent += `[${ts}] ${msg}\n`;
    logEl.scrollTop = logEl.scrollHeight;
  }

  function suggestOutput(bam, region) {
    if (!bam) return '';
    const slug = (region || 'region').replace(/[^A-Za-z0-9._-]/g, '_');
    const dot = bam.lastIndexOf('.');
    const stem = dot > 0 ? bam.slice(0, dot) : bam;
    return `${stem}.${slug}.bam`;
  }

  async function refreshIndexStatus() {
    if (!s.bamPath) return;
    try {
      const has = await api('bam_tools_index_status', { path: s.bamPath });
      s.indexed = has;
      statusEl.textContent = has
        ? `✓ ${t('utility.bam_tools.index_present')}`
        : `⚠ ${t('utility.bam_tools.index_missing')}`;
      statusEl.className = 'bam-tools-index-status ' + (has ? 'ok' : 'warn');
      extractCard.hidden = !has;
    } catch (e) {
      log(`index_status error: ${e?.message || e}`);
    }
  }

  async function openBam() {
    const paths = await api('select_files', { multiple: false });
    if (!paths || !paths[0]) return;
    s.bamPath = paths[0];
    pathEl.textContent = s.bamPath;
    pathEl.title = s.bamPath;
    indexCard.hidden = false;

    chromSelect.innerHTML = `<option value="">—</option>`;
    try {
      const refs = await api('bam_tools_header_references', { path: s.bamPath });
      s.refs = refs;
      for (const r of refs) {
        const opt = document.createElement('option');
        opt.value = r.name;
        opt.textContent = `${r.name} (${r.length.toLocaleString()} bp)`;
        chromSelect.appendChild(opt);
      }
      log(`${t('utility.bam_tools.loaded_refs', { count: refs.length })}`);
    } catch (e) {
      log(`header error: ${e?.message || e}`);
    }
    await refreshIndexStatus();
  }

  async function buildIndex() {
    if (!s.bamPath) return;
    log(t('utility.bam_tools.indexing'));
    try {
      const res = await api('bam_tools_index', { path: s.bamPath });
      log(`✓ ${t('utility.bam_tools.index_written', { path: res.bai })}`);
      await refreshIndexStatus();
    } catch (e) {
      log(`✗ ${e?.message || e}`);
    }
  }

  async function extract() {
    if (!s.bamPath) return;
    const region = regionInput.value.trim();
    if (!region) {
      log(t('utility.bam_tools.region_required'));
      return;
    }
    const output = outputInput.value.trim() || suggestOutput(s.bamPath, region);
    log(t('utility.bam_tools.extracting', { region }));
    try {
      const res = await api('bam_tools_extract_region', {
        path: s.bamPath,
        region,
        output,
      });
      log(`✓ ${t('utility.bam_tools.extract_done', { count: res.records_written, path: res.output })}`);
    } catch (e) {
      log(`✗ ${e?.message || e}`);
    }
  }

  async function browseOutputDir() {
    const dir = await api('select_directory', {});
    if (!dir) return;
    const region = regionInput.value.trim();
    const slug = (region || 'region').replace(/[^A-Za-z0-9._-]/g, '_');
    const stem = s.bamPath ? s.bamPath.split(/[\\/]/).pop().replace(/\.[^.]+$/, '') : 'output';
    outputInput.value = `${dir.replace(/[\\/]$/, '')}/${stem}.${slug}.bam`;
  }

  chromSelect.addEventListener('change', () => {
    if (chromSelect.value && !regionInput.value) {
      regionInput.value = chromSelect.value;
    }
  });

  regionInput.addEventListener('blur', () => {
    if (s.bamPath && !outputInput.value) {
      outputInput.value = suggestOutput(s.bamPath, regionInput.value.trim());
    }
  });

  if (content._bamToolsClickHandler) {
    content.removeEventListener('click', content._bamToolsClickHandler);
  }
  content._bamToolsClickHandler = (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'bam-open') openBam();
    else if (act === 'bam-index') buildIndex();
    else if (act === 'bam-extract') extract();
    else if (act === 'bam-output-browse') browseOutputDir();
  };
  content.addEventListener('click', content._bamToolsClickHandler);
}
