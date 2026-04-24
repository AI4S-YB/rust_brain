import { t } from '../../core/i18n-helpers.js';
import { samplesApi } from '../../api/samples.js';
import { inputsApi } from '../../api/inputs.js';
import { filesApi } from '../../api/files.js';
import { escapeHtml } from '../../ui/escape.js';
import { confirmModal, alertModal, showToast, promptModal } from '../../ui/modal.js';
import { formatBytes } from '../run-result.js';

const viewState = {
  samples: [],
  inputs: [],
  search: '',
};

function inputById(id) {
  return viewState.inputs.find(i => i.id === id);
}

function inputChipsHtml(sampleId, inputIds) {
  if (!inputIds?.length) return `<em class="sample-no-inputs">${escapeHtml(t('samples.no_inputs'))}</em>`;
  return inputIds
    .map(id => {
      const rec = inputById(id);
      const label = rec ? rec.display_name : id;
      const missing = rec?.missing ? ' sample-input-missing' : '';
      return `
        <span class="sample-input-chip${missing}" title="${escapeHtml(rec?.path || id)}">
          ${escapeHtml(label)}
          <button type="button" class="sample-chip-x"
                  data-act="samples-unlink-input"
                  data-sample-id="${escapeHtml(sampleId)}"
                  data-input-id="${escapeHtml(id)}"
                  title="${escapeHtml(t('samples.unlink_input'))}">×</button>
        </span>`;
    })
    .join('');
}

function renderRow(s) {
  return `
    <tr data-sample-id="${escapeHtml(s.id)}">
      <td class="samples-col-name">
        <input type="text" class="sample-name-edit"
               data-sample-id="${escapeHtml(s.id)}"
               data-field="name"
               value="${escapeHtml(s.name)}"/>
      </td>
      <td class="samples-col-group">
        <input type="text" class="sample-name-edit"
               data-sample-id="${escapeHtml(s.id)}"
               data-field="group"
               value="${escapeHtml(s.group || '')}"
               placeholder="${escapeHtml(t('samples.group_placeholder'))}"/>
      </td>
      <td class="samples-col-condition">
        <input type="text" class="sample-name-edit"
               data-sample-id="${escapeHtml(s.id)}"
               data-field="condition"
               value="${escapeHtml(s.condition || '')}"
               placeholder="${escapeHtml(t('samples.condition_placeholder'))}"/>
      </td>
      <td>${s.paired ? `<span class="kind-pill kind-fastq">PE</span>` : `<span class="kind-pill kind-other">SE</span>`}</td>
      <td class="samples-col-inputs">${inputChipsHtml(s.id, s.inputs)}</td>
      <td class="samples-col-actions">
        <button type="button" class="btn btn-ghost btn-sm"
                data-act="samples-add-input"
                data-sample-id="${escapeHtml(s.id)}"
                title="${escapeHtml(t('samples.add_input'))}">
          <i data-lucide="file-plus"></i>
        </button>
        <button type="button" class="btn btn-ghost btn-sm"
                data-act="samples-delete-row"
                data-sample-id="${escapeHtml(s.id)}"
                title="${escapeHtml(t('samples.delete'))}">
          <i data-lucide="trash-2"></i>
        </button>
      </td>
    </tr>`;
}

function filteredSamples() {
  const q = viewState.search.trim().toLowerCase();
  if (!q) return viewState.samples;
  return viewState.samples.filter(s => {
    const hay = [s.name, s.group || '', s.condition || ''].join('\n').toLowerCase();
    return hay.includes(q);
  });
}

function renderTable() {
  const tbody = document.getElementById('samples-table-body');
  if (!tbody) return;
  const rows = filteredSamples();
  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" class="samples-empty">${escapeHtml(t('samples.empty'))}</td></tr>`;
  } else {
    tbody.innerHTML = rows.map(renderRow).join('');
  }
  const count = document.getElementById('samples-count');
  if (count) count.textContent = t('samples.count_label', { n: rows.length });
  if (window.lucide) window.lucide.createIcons();
}

async function loadAll() {
  try {
    const [samples, inputs] = await Promise.all([samplesApi.list(), inputsApi.list()]);
    viewState.samples = (samples || []).sort((a, b) => a.name.localeCompare(b.name));
    viewState.inputs = inputs || [];
  } catch (err) {
    viewState.samples = [];
    viewState.inputs = [];
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
  renderTable();
}

async function pickInputIds({ title, preselected = [] }) {
  const selected = new Set(preselected);
  const fastqInputs = viewState.inputs.filter(i => i.kind === 'Fastq' || i.kind === 'CountsMatrix');
  if (fastqInputs.length === 0) {
    alertModal({ title: t('samples.no_candidates_title'), message: t('samples.no_candidates_message') });
    return null;
  }

  return new Promise((resolve) => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    const rowsHtml = fastqInputs.map(i => `
      <label class="samples-pick-row">
        <input type="checkbox" value="${escapeHtml(i.id)}" ${selected.has(i.id) ? 'checked' : ''}/>
        <span class="kind-pill kind-${String(i.kind).toLowerCase()}">${escapeHtml(i.kind)}</span>
        <span class="samples-pick-name">${escapeHtml(i.display_name)}</span>
        <span class="samples-pick-size">${escapeHtml(formatBytes(i.size_bytes || 0))}</span>
      </label>
    `).join('');
    backdrop.innerHTML = `
      <div class="modal samples-pick-modal" role="dialog" aria-modal="true">
        <h3 class="modal-title">${escapeHtml(title)}</h3>
        <div class="samples-pick-list">${rowsHtml}</div>
        <div class="modal-actions">
          <button type="button" class="btn btn-secondary" data-role="cancel">${escapeHtml(t('common.cancel'))}</button>
          <button type="button" class="btn btn-primary" data-role="ok">${escapeHtml(t('common.ok'))}</button>
        </div>
      </div>`;
    document.body.appendChild(backdrop);
    const finish = (v) => { backdrop.remove(); resolve(v); };
    backdrop.querySelector('[data-role=cancel]').addEventListener('click', () => finish(null));
    backdrop.querySelector('[data-role=ok]').addEventListener('click', () => {
      const ids = [...backdrop.querySelectorAll('input[type=checkbox]:checked')].map(x => x.value);
      finish(ids);
    });
    backdrop.addEventListener('click', (e) => { if (e.target === backdrop) finish(null); });
  });
}

async function handleNewSample() {
  const name = await promptModal({
    title: t('samples.new_title'),
    message: t('samples.new_prompt'),
    placeholder: 'sample_01',
  });
  if (!name) return;
  const inputIds = await pickInputIds({ title: t('samples.pick_inputs_title') });
  if (inputIds === null) return;
  try {
    await samplesApi.create(name.trim(), { inputIds });
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

function handleDownloadTemplate() {
  const tsv = [
    'sample_id\tgroup\tcondition\tr1\tr2\tnotes',
    'sample_01\ttreat\tIFN\tinput/sample_01_R1.fastq.gz\tinput/sample_01_R2.fastq.gz\treplicate 1',
    'sample_02\ttreat\tIFN\tinput/sample_02_R1.fastq.gz\tinput/sample_02_R2.fastq.gz\treplicate 2',
    'sample_03\tctrl\tDMSO\tinput/sample_03_R1.fastq.gz\tinput/sample_03_R2.fastq.gz\treplicate 1',
    'sample_04\tctrl\tDMSO\tinput/sample_04_R1.fastq.gz\tinput/sample_04_R2.fastq.gz\treplicate 2',
    '',
  ].join('\n');
  const blob = new Blob([tsv], { type: 'text/tab-separated-values;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = 'sample_sheet_template.tsv';
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

async function handleImportTsv() {
  let picked;
  try {
    picked = await filesApi.selectFiles({ multiple: false });
  } catch {
    picked = null;
  }
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (!path) return;
  try {
    const result = await samplesApi.importTsv(path);
    const created = result?.created?.length || 0;
    const errs = result?.errors || [];
    showToast({
      title: t('samples.import_toast_title'),
      message: t('samples.import_toast_message', { n: created }),
    });
    if (errs.length) {
      alertModal({ title: t('status.error_prefix'), message: errs.join('\n') });
    }
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleAutoPair() {
  try {
    const created = await samplesApi.autoPair();
    showToast({
      title: t('samples.auto_pair_toast_title'),
      message: t('samples.auto_pair_toast_message', { n: created?.length ?? 0 }),
    });
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleDelete(id) {
  const ok = await confirmModal({
    title: t('samples.delete_confirm_title'),
    message: t('samples.delete_confirm_message'),
    okLabel: t('samples.delete'),
  });
  if (!ok) return;
  try {
    await samplesApi.delete(id);
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleAddInput(sampleId) {
  const sample = viewState.samples.find(s => s.id === sampleId);
  const current = sample?.inputs || [];
  const picked = await pickInputIds({
    title: t('samples.add_input_title'),
    preselected: current,
  });
  if (picked === null) return;
  try {
    await samplesApi.update(sampleId, { inputs: picked });
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleUnlinkInput(sampleId, inputId) {
  const sample = viewState.samples.find(s => s.id === sampleId);
  if (!sample) return;
  const next = sample.inputs.filter(id => id !== inputId);
  try {
    await samplesApi.update(sampleId, { inputs: next });
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleFieldEdit(sampleId, field, value) {
  const patch = {};
  const v = (value || '').trim();
  if (field === 'name') {
    if (!v) return loadAll();
    patch.name = v;
  } else if (field === 'group') {
    patch.group = v;
  } else if (field === 'condition') {
    patch.condition = v;
  } else {
    return;
  }
  try {
    await samplesApi.update(sampleId, patch);
    const rec = viewState.samples.find(s => s.id === sampleId);
    if (rec) Object.assign(rec, patch);
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
    await loadAll();
  }
}

function bindEvents(container) {
  container.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    switch (btn.dataset.act) {
      case 'samples-new':           handleNewSample(); break;
      case 'samples-auto-pair':     handleAutoPair(); break;
      case 'samples-import-tsv':    handleImportTsv(); break;
      case 'samples-download-template': handleDownloadTemplate(); break;
      case 'samples-delete-row':    handleDelete(btn.dataset.sampleId); break;
      case 'samples-add-input':     handleAddInput(btn.dataset.sampleId); break;
      case 'samples-unlink-input':  handleUnlinkInput(btn.dataset.sampleId, btn.dataset.inputId); break;
      case 'samples-refresh':       loadAll(); break;
    }
  });
  container.addEventListener('input', (e) => {
    if (e.target.id === 'samples-search') {
      viewState.search = e.target.value;
      renderTable();
    }
  });
  container.addEventListener('blur', (e) => {
    if (e.target.classList?.contains('sample-name-edit')) {
      handleFieldEdit(e.target.dataset.sampleId, e.target.dataset.field, e.target.value);
    }
  }, true);
  container.addEventListener('keydown', (e) => {
    if (e.target.classList?.contains('sample-name-edit') && e.key === 'Enter') {
      e.preventDefault();
      e.target.blur();
    }
  });
}

export function renderSamplesView(container) {
  container.innerHTML = `
    <div class="module-view samples-view">
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: rgba(45,134,89,0.12); color: #2d8659;">
          <i data-lucide="users"></i>
        </div>
        <div>
          <h1 class="module-title">${escapeHtml(t('samples.title'))}</h1>
          <p class="module-desc">${escapeHtml(t('samples.subtitle'))}</p>
        </div>
      </div>

      <div class="card inputs-toolbar">
        <div class="inputs-toolbar-row">
          <button type="button" class="btn btn-primary" data-act="samples-new">
            <i data-lucide="plus"></i> ${escapeHtml(t('samples.new'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="samples-auto-pair">
            <i data-lucide="git-merge"></i> ${escapeHtml(t('samples.auto_pair'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="samples-import-tsv">
            <i data-lucide="file-spreadsheet"></i> ${escapeHtml(t('samples.import_tsv'))}
          </button>
          <button type="button" class="btn btn-ghost btn-sm" data-act="samples-download-template"
                  title="${escapeHtml(t('samples.download_template_hint'))}">
            <i data-lucide="download"></i> ${escapeHtml(t('samples.download_template'))}
          </button>
          <button type="button" class="btn btn-secondary" data-act="samples-refresh">
            <i data-lucide="refresh-cw"></i> ${escapeHtml(t('common.refresh'))}
          </button>
          <input type="search" id="samples-search" class="inputs-search"
                 placeholder="${escapeHtml(t('samples.search_placeholder'))}"/>
        </div>
        <div class="inputs-summary">
          <span id="samples-count"></span>
        </div>
      </div>

      <div class="card inputs-table-card">
        <table class="inputs-table">
          <thead>
            <tr>
              <th>${escapeHtml(t('samples.col_name'))}</th>
              <th>${escapeHtml(t('samples.col_group'))}</th>
              <th>${escapeHtml(t('samples.col_condition'))}</th>
              <th>${escapeHtml(t('samples.col_layout'))}</th>
              <th>${escapeHtml(t('samples.col_inputs'))}</th>
              <th class="inputs-col-actions">${escapeHtml(t('samples.col_actions'))}</th>
            </tr>
          </thead>
          <tbody id="samples-table-body">
            <tr><td colspan="6">${escapeHtml(t('common.loading'))}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  `;
  bindEvents(container);
  loadAll();
  if (window.lucide) window.lucide.createIcons();
}
