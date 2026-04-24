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
  selected: new Set(),
};

const PAIR_PATTERN_STORAGE = 'rustbrain.samples.pairPatterns';
const DEFAULT_PAIR_PATTERNS = [
  { r1: '_R1_001', r2: '_R2_001' },
  { r1: '_R1', r2: '_R2' },
  { r1: '.R1', r2: '.R2' },
  { r1: '-R1', r2: '-R2' },
  { r1: 'R1', r2: 'R2' },
  { r1: '_1', r2: '_2' },
  { r1: '.1', r2: '.2' },
  { r1: '-1', r2: '-2' },
];

function inputById(id) {
  return viewState.inputs.find(i => i.id === id);
}

function inputDisplayName(id) {
  return inputById(id)?.display_name || id;
}

function patternLabel(pattern) {
  if (!pattern?.r1 || !pattern?.r2) return '';
  return `${pattern.r1} / ${pattern.r2}`;
}

function splitPatternList(value) {
  return String(value || '')
    .split(/[,\n;]+/)
    .map(x => x.trim())
    .filter(Boolean);
}

function parsePatternFields(r1Text, r2Text) {
  const r1 = splitPatternList(r1Text);
  const r2 = splitPatternList(r2Text);
  if (r1.length !== r2.length) {
    throw new Error(t('samples.auto_pair_pattern_error'));
  }
  return r1.map((x, i) => ({ r1: x, r2: r2[i] }));
}

function loadPairPatterns() {
  try {
    const raw = window.localStorage?.getItem(PAIR_PATTERN_STORAGE);
    const parsed = raw ? JSON.parse(raw) : null;
    if (Array.isArray(parsed) && parsed.every(p => p?.r1 && p?.r2)) return parsed;
  } catch {
    // Ignore invalid stored patterns.
  }
  return DEFAULT_PAIR_PATTERNS;
}

function savePairPatterns(patterns) {
  try {
    window.localStorage?.setItem(PAIR_PATTERN_STORAGE, JSON.stringify(patterns));
  } catch {
    // Storage can be unavailable in restricted webviews.
  }
}

function patternsToText(patterns, mate) {
  return patterns.map(p => p[mate]).join(', ');
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
  const checked = viewState.selected.has(s.id) ? 'checked' : '';
  return `
    <tr data-sample-id="${escapeHtml(s.id)}">
      <td class="inputs-col-check">
        <input type="checkbox" class="samples-row-check" data-sample-id="${escapeHtml(s.id)}" ${checked}/>
      </td>
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
    tbody.innerHTML = `<tr><td colspan="7" class="samples-empty">${escapeHtml(t('samples.empty'))}</td></tr>`;
  } else {
    tbody.innerHTML = rows.map(renderRow).join('');
  }
  const count = document.getElementById('samples-count');
  if (count) count.textContent = t('samples.count_label', { n: rows.length });
  renderSelectionSummary(rows);
  if (window.lucide) window.lucide.createIcons();
}

function renderSelectionSummary(rows = filteredSamples()) {
  const visibleIds = rows.map(s => s.id);
  const visibleSelected = visibleIds.filter(id => viewState.selected.has(id)).length;
  const selectAll = document.getElementById('samples-select-all');
  if (selectAll) {
    selectAll.checked = rows.length > 0 && visibleSelected === rows.length;
    selectAll.indeterminate = visibleSelected > 0 && visibleSelected < rows.length;
  }
  const deleteBtn = document.querySelector('[data-act="samples-delete-selected"]');
  if (deleteBtn) deleteBtn.disabled = viewState.selected.size === 0;
  const selectedEl = document.getElementById('samples-selected-count');
  if (selectedEl) {
    selectedEl.textContent = viewState.selected.size > 0
      ? t('samples.selected_label', { n: viewState.selected.size })
      : '';
  }
}

async function loadAll() {
  try {
    const [samples, inputs] = await Promise.all([samplesApi.list(), inputsApi.list()]);
    viewState.samples = (samples || []).sort((a, b) => a.name.localeCompare(b.name));
    viewState.inputs = inputs || [];
    const alive = new Set(viewState.samples.map(s => s.id));
    [...viewState.selected].forEach(id => { if (!alive.has(id)) viewState.selected.delete(id); });
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
    let settled = false;
    const finish = (v) => {
      if (settled) return;
      settled = true;
      backdrop.remove();
      resolve(v);
    };
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

function previewAutoPairModal() {
  const initialPatterns = loadPairPatterns();
  return new Promise((resolve) => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    backdrop.innerHTML = `
      <div class="modal samples-auto-pair-modal" role="dialog" aria-modal="true">
        <h3 class="modal-title">${escapeHtml(t('samples.auto_pair_title'))}</h3>
        <div class="samples-pattern-grid">
          <label class="samples-pattern-field">
            <span>${escapeHtml(t('samples.auto_pair_r1_patterns'))}</span>
            <textarea rows="2" data-role="r1-patterns">${escapeHtml(patternsToText(initialPatterns, 'r1'))}</textarea>
          </label>
          <label class="samples-pattern-field">
            <span>${escapeHtml(t('samples.auto_pair_r2_patterns'))}</span>
            <textarea rows="2" data-role="r2-patterns">${escapeHtml(patternsToText(initialPatterns, 'r2'))}</textarea>
          </label>
        </div>
        <div class="samples-pair-error" data-role="pair-error"></div>
        <div class="samples-pair-preview">
          <table class="samples-pair-table">
            <thead>
              <tr>
                <th class="inputs-col-check">
                  <input type="checkbox" data-role="pair-select-all" aria-label="${escapeHtml(t('samples.select_all'))}"/>
                </th>
                <th>${escapeHtml(t('samples.auto_pair_col_name'))}</th>
                <th>${escapeHtml(t('samples.auto_pair_col_layout'))}</th>
                <th>${escapeHtml(t('samples.auto_pair_col_r1'))}</th>
                <th>${escapeHtml(t('samples.auto_pair_col_r2'))}</th>
                <th>${escapeHtml(t('samples.auto_pair_col_pattern'))}</th>
              </tr>
            </thead>
            <tbody data-role="pair-preview-body">
              <tr><td colspan="6">${escapeHtml(t('common.loading'))}</td></tr>
            </tbody>
          </table>
        </div>
        <div class="modal-actions">
          <button type="button" class="btn btn-secondary" data-role="cancel">${escapeHtml(t('common.cancel'))}</button>
          <button type="button" class="btn btn-secondary" data-role="refresh">${escapeHtml(t('samples.auto_pair_refresh'))}</button>
          <button type="button" class="btn btn-primary" data-role="ok" disabled>${escapeHtml(t('samples.auto_pair_confirm'))}</button>
        </div>
      </div>`;
    document.body.appendChild(backdrop);

    const r1El = backdrop.querySelector('[data-role="r1-patterns"]');
    const r2El = backdrop.querySelector('[data-role="r2-patterns"]');
    const errEl = backdrop.querySelector('[data-role="pair-error"]');
    const bodyEl = backdrop.querySelector('[data-role="pair-preview-body"]');
    const okBtn = backdrop.querySelector('[data-role="ok"]');
    const selectAllEl = backdrop.querySelector('[data-role="pair-select-all"]');
    let previews = [];
    let refreshToken = 0;
    let refreshTimer = null;
    let settled = false;

    const setError = (message, kind = '') => {
      errEl.textContent = message || '';
      if (kind) errEl.dataset.errorKind = kind;
      else delete errEl.dataset.errorKind;
    };

    const finish = (value) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(refreshTimer);
      backdrop.remove();
      resolve(value);
    };

    const selectedRows = () => {
      return [...backdrop.querySelectorAll('.samples-pair-row-check:checked')]
        .map(cb => {
          const idx = Number(cb.dataset.previewIndex);
          const preview = previews[idx];
          const nameEl = backdrop.querySelector(`.samples-pair-name[data-preview-index="${idx}"]`);
          const name = (nameEl?.value || preview?.name || '').trim();
          return preview ? { ...preview, name } : null;
        })
        .filter(row => row && row.name && row.inputs?.length);
    };

    const updateModalState = () => {
      const checks = [...backdrop.querySelectorAll('.samples-pair-row-check')];
      const checkedCount = checks.filter(x => x.checked).length;
      const selected = selectedRows().length;
      if (selected > 0 && errEl.dataset.errorKind === 'selection') setError('');
      okBtn.disabled = selected === 0 || (Boolean(errEl.textContent) && errEl.dataset.errorKind !== 'selection');
      if (selectAllEl) {
        selectAllEl.checked = checks.length > 0 && checkedCount === checks.length;
        selectAllEl.indeterminate = checkedCount > 0 && checkedCount < checks.length;
      }
    };

    const renderPreview = () => {
      if (!previews.length) {
        bodyEl.innerHTML = `<tr><td colspan="6" class="samples-empty">${escapeHtml(t('samples.auto_pair_empty'))}</td></tr>`;
        updateModalState();
        return;
      }
      bodyEl.innerHTML = previews.map((row, idx) => {
        const inputs = row.inputs || [];
        const r1 = inputById(inputs[0]);
        const r2 = inputById(inputs[1]);
        const checked = row.paired ? 'checked' : '';
        return `
          <tr>
            <td class="inputs-col-check">
              <input type="checkbox" class="samples-pair-row-check" data-preview-index="${idx}" ${checked}/>
            </td>
            <td>
              <input type="text" class="sample-name-edit samples-pair-name"
                     data-preview-index="${idx}"
                     value="${escapeHtml(row.name || '')}"/>
            </td>
            <td>${row.paired ? `<span class="kind-pill kind-fastq">PE</span>` : `<span class="kind-pill kind-other">SE</span>`}</td>
            <td class="samples-pair-file" title="${escapeHtml(r1?.path || inputs[0] || '')}">${escapeHtml(inputDisplayName(inputs[0]))}</td>
            <td class="samples-pair-file" title="${escapeHtml(r2?.path || inputs[1] || '')}">${inputs[1] ? escapeHtml(inputDisplayName(inputs[1])) : '<span class="samples-pair-muted">-</span>'}</td>
            <td class="samples-pair-pattern">${escapeHtml(patternLabel(row.pattern))}</td>
          </tr>`;
      }).join('');
      updateModalState();
    };

    const refreshPreview = async () => {
      const token = ++refreshToken;
      let patterns;
      try {
        patterns = parsePatternFields(r1El.value, r2El.value);
        setError('');
      } catch (err) {
        previews = [];
        setError(err.message || String(err), 'pattern');
        renderPreview();
        return;
      }
      bodyEl.innerHTML = `<tr><td colspan="6">${escapeHtml(t('common.loading'))}</td></tr>`;
      updateModalState();
      try {
        const rows = await samplesApi.previewAutoPair(patterns);
        if (token !== refreshToken) return;
        previews = rows || [];
        renderPreview();
      } catch (err) {
        if (token !== refreshToken) return;
        previews = [];
        setError(String(err), 'preview');
        renderPreview();
      }
    };

    const scheduleRefresh = () => {
      window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(refreshPreview, 220);
    };

    r1El.addEventListener('input', scheduleRefresh);
    r2El.addEventListener('input', scheduleRefresh);
    backdrop.querySelector('[data-role="refresh"]').addEventListener('click', refreshPreview);
    backdrop.querySelector('[data-role="cancel"]').addEventListener('click', () => finish(null));
    backdrop.querySelector('[data-role="ok"]').addEventListener('click', () => {
      try {
        const patterns = parsePatternFields(r1El.value, r2El.value);
        const selected = selectedRows();
        if (!selected.length) {
          setError(t('samples.auto_pair_no_selected'), 'selection');
          updateModalState();
          return;
        }
        savePairPatterns(patterns);
        finish({ patterns, selected });
      } catch (err) {
        setError(err.message || String(err), 'pattern');
        updateModalState();
      }
    });
    selectAllEl.addEventListener('change', () => {
      backdrop.querySelectorAll('.samples-pair-row-check').forEach(cb => { cb.checked = selectAllEl.checked; });
      updateModalState();
    });
    bodyEl.addEventListener('change', (e) => {
      if (e.target.classList?.contains('samples-pair-row-check')) updateModalState();
    });
    bodyEl.addEventListener('input', (e) => {
      if (e.target.classList?.contains('samples-pair-name')) updateModalState();
    });
    backdrop.addEventListener('click', (e) => { if (e.target === backdrop) finish(null); });
    refreshPreview();
  });
}

async function handleAutoPair() {
  const plan = await previewAutoPairModal();
  if (!plan) return;
  const created = [];
  const errors = [];
  for (const row of plan.selected) {
    try {
      const rec = await samplesApi.create(row.name, { inputIds: row.inputs });
      created.push(rec);
    } catch (err) {
      errors.push(`${row.name}: ${err}`);
    }
  }
  try {
    showToast({
      title: t('samples.auto_pair_toast_title'),
      message: t('samples.auto_pair_toast_message', { n: created.length }),
    });
    if (errors.length) {
      alertModal({ title: t('status.error_prefix'), message: errors.join('\n') });
    }
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
    viewState.selected.delete(id);
    await loadAll();
  } catch (err) {
    alertModal({ title: t('status.error_prefix'), message: String(err) });
  }
}

async function handleDeleteSelected() {
  if (viewState.selected.size === 0) return;
  const ids = [...viewState.selected];
  const ok = await confirmModal({
    title: t('samples.delete_selected_title'),
    message: t('samples.delete_selected_message', { n: ids.length }),
    okLabel: t('samples.delete'),
  });
  if (!ok) return;
  const errors = [];
  for (const id of ids) {
    try {
      await samplesApi.delete(id);
      viewState.selected.delete(id);
    } catch (err) {
      errors.push(`${id}: ${err}`);
    }
  }
  showToast({
    title: t('samples.deleted_toast_title'),
    message: t('samples.deleted_toast_message', { n: ids.length - errors.length }),
  });
  if (errors.length) {
    alertModal({ title: t('status.error_prefix'), message: errors.join('\n') });
  }
  await loadAll();
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

const fieldEditTokens = new Map(); // key=`${sampleId}:${field}` → latest token
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
  const tokenKey = `${sampleId}:${field}`;
  const token = (fieldEditTokens.get(tokenKey) || 0) + 1;
  fieldEditTokens.set(tokenKey, token);
  try {
    await samplesApi.update(sampleId, patch);
    if (fieldEditTokens.get(tokenKey) !== token) return; // stale response
    const rec = viewState.samples.find(s => s.id === sampleId);
    if (rec) Object.assign(rec, patch);
  } catch (err) {
    if (fieldEditTokens.get(tokenKey) !== token) return;
    alertModal({ title: t('status.error_prefix'), message: String(err) });
    await loadAll();
  }
}

function bindEvents(container) {
  if (container.dataset.samplesEventsBound === 'true') return;
  container.dataset.samplesEventsBound = 'true';

  container.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-act]');
    if (!btn) return;
    switch (btn.dataset.act) {
      case 'samples-new':           handleNewSample(); break;
      case 'samples-auto-pair':     handleAutoPair(); break;
      case 'samples-import-tsv':    handleImportTsv(); break;
      case 'samples-download-template': handleDownloadTemplate(); break;
      case 'samples-delete-selected': handleDeleteSelected(); break;
      case 'samples-delete-row':    handleDelete(btn.dataset.sampleId); break;
      case 'samples-add-input':     handleAddInput(btn.dataset.sampleId); break;
      case 'samples-unlink-input':  handleUnlinkInput(btn.dataset.sampleId, btn.dataset.inputId); break;
      case 'samples-refresh':       loadAll(); break;
    }
  });
  container.addEventListener('change', (e) => {
    if (e.target.id === 'samples-select-all') {
      const rows = filteredSamples();
      if (e.target.checked) rows.forEach(s => viewState.selected.add(s.id));
      else rows.forEach(s => viewState.selected.delete(s.id));
      renderTable();
    } else if (e.target.classList?.contains('samples-row-check')) {
      const id = e.target.dataset.sampleId;
      if (e.target.checked) viewState.selected.add(id);
      else viewState.selected.delete(id);
      renderSelectionSummary();
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
          <div class="inputs-toolbar-spacer"></div>
          <button type="button" class="btn btn-danger" data-act="samples-delete-selected" disabled>
            <i data-lucide="trash-2"></i> ${escapeHtml(t('samples.delete_selected'))}
          </button>
        </div>
        <div class="inputs-summary">
          <span id="samples-count"></span>
          <span id="samples-selected-count"></span>
        </div>
      </div>

      <div class="card inputs-table-card">
        <table class="inputs-table">
          <thead>
            <tr>
              <th class="inputs-col-check">
                <input type="checkbox" id="samples-select-all" aria-label="${escapeHtml(t('samples.select_all'))}"/>
              </th>
              <th>${escapeHtml(t('samples.col_name'))}</th>
              <th>${escapeHtml(t('samples.col_group'))}</th>
              <th>${escapeHtml(t('samples.col_condition'))}</th>
              <th>${escapeHtml(t('samples.col_layout'))}</th>
              <th>${escapeHtml(t('samples.col_inputs'))}</th>
              <th class="inputs-col-actions">${escapeHtml(t('samples.col_actions'))}</th>
            </tr>
          </thead>
          <tbody id="samples-table-body">
            <tr><td colspan="7">${escapeHtml(t('common.loading'))}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  `;
  bindEvents(container);
  loadAll();
  if (window.lucide) window.lucide.createIcons();
}
