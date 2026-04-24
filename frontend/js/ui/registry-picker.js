/**
 * Shared registry picker UI helpers. Used by module forms (P4) so users can
 * pick registered Inputs / Assets / Samples instead of pasting paths.
 *
 * Each helper initialises a host element (typically a wrapper div the view
 * dropped into its form) and handles the fetch + render + on-change logic.
 * The host element is expected to carry the following data-* attributes:
 *
 *   <div class="registry-picker"
 *        data-kind="asset"           // asset | input | sample
 *        data-asset-kind="StarIndex" // required for kind=asset
 *        data-target-name="genome_dir"// form input[name=…] to populate
 *        data-lineage-key="asset"    // asset | input (how to emit lineage)
 *   >…</div>
 *
 * The picker keeps its chosen id on the host as `dataset.selectedId`.
 * Callers read it at submit time via `pickerSelection(form)`.
 */

import { assetsApi } from '../api/assets.js';
import { inputsApi } from '../api/inputs.js';
import { samplesApi } from '../api/samples.js';
import { escapeHtml } from './escape.js';
import { t } from '../core/i18n-helpers.js';
import { formatBytes } from '../modules/run-result.js';

const assetCache = { rows: null, at: 0 };
const inputCache = { rows: null, at: 0 };
const sampleCache = { rows: null, at: 0 };
const CACHE_TTL_MS = 30_000;

function now() { return Date.now(); }

async function loadAssets(force = false) {
  if (!force && assetCache.rows && now() - assetCache.at < CACHE_TTL_MS) return assetCache.rows;
  assetCache.rows = await assetsApi.list() || [];
  assetCache.at = now();
  return assetCache.rows;
}
async function loadInputs(force = false) {
  if (!force && inputCache.rows && now() - inputCache.at < CACHE_TTL_MS) return inputCache.rows;
  inputCache.rows = await inputsApi.list() || [];
  inputCache.at = now();
  return inputCache.rows;
}
async function loadSamples(force = false) {
  if (!force && sampleCache.rows && now() - sampleCache.at < CACHE_TTL_MS) return sampleCache.rows;
  sampleCache.rows = await samplesApi.list() || [];
  sampleCache.at = now();
  return sampleCache.rows;
}

export function invalidatePickerCache() {
  assetCache.rows = null;
  inputCache.rows = null;
  sampleCache.rows = null;
}

/**
 * Attach an Asset picker dropdown to the host. When the user picks an asset:
 *  - if `onPick(asset | null)` is provided, call it (view owns the effect)
 *  - otherwise, set the sibling `input[name=<target>]` to the asset path
 */
export async function attachAssetPicker(host, onPick = null) {
  const kind = host.dataset.assetKind;
  const target = host.dataset.targetName;
  const form = host.closest('form');
  const targetInput = form?.querySelector(`input[name="${target}"], textarea[name="${target}"]`);

  host.innerHTML = `
    <label class="form-label registry-picker-label">
      <i data-lucide="package" style="width:13px;height:13px;vertical-align:-2px;margin-right:4px"></i>
      ${escapeHtml(t('registry_picker.asset_label'))}
    </label>
    <select class="form-select registry-picker-select">
      <option value="">${escapeHtml(t('registry_picker.none'))}</option>
      <option value="__loading" disabled>${escapeHtml(t('common.loading'))}</option>
    </select>
    <p class="registry-picker-hint">${escapeHtml(t('registry_picker.asset_hint'))}</p>
  `;
  const select = host.querySelector('select');

  try {
    const assets = (await loadAssets()).filter(a => a.kind === kind);
    select.innerHTML = `<option value="">${escapeHtml(t('registry_picker.none'))}</option>` +
      assets.map(a => `
        <option value="${escapeHtml(a.id)}" data-path="${escapeHtml(a.path)}">
          ${escapeHtml(a.display_name)} — ${escapeHtml(formatBytes(a.size_bytes || 0))}
        </option>`).join('');
    if (assets.length === 0) {
      select.innerHTML += `<option value="" disabled>${escapeHtml(t('registry_picker.asset_empty'))}</option>`;
    }
  } catch {
    select.innerHTML = `<option value="">${escapeHtml(t('registry_picker.none'))}</option>`;
  }

  select.addEventListener('change', () => {
    const id = select.value;
    const opt = select.selectedOptions[0];
    host.dataset.selectedId = id || '';
    const asset = id ? assetCache.rows?.find(a => a.id === id) : null;
    if (onPick) {
      onPick(asset);
    } else if (targetInput && asset?.path) {
      targetInput.value = asset.path;
      targetInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
  });

  if (window.lucide) window.lucide.createIcons();
}

/**
 * Attach an Input picker dropdown. Like the Asset picker but draws from the
 * `Inputs` registry (raw user files) filtered by `data-input-kind` on the host
 * (e.g. "Fasta", "Gtf", "Gff", "Fastq"). Optional `onPick(input | null)` mirrors
 * attachAssetPicker; default behavior fills the sibling input[name=target].
 */
export async function attachInputPicker(host, onPick = null) {
  const kind = host.dataset.inputKind;
  const target = host.dataset.targetName;
  const form = host.closest('form');
  const targetInput = form?.querySelector(`input[name="${target}"], textarea[name="${target}"]`);

  host.innerHTML = `
    <label class="form-label registry-picker-label">
      <i data-lucide="database" style="width:13px;height:13px;vertical-align:-2px;margin-right:4px"></i>
      ${escapeHtml(t('registry_picker.input_label'))}
    </label>
    <select class="form-select registry-picker-select">
      <option value="">${escapeHtml(t('registry_picker.none'))}</option>
    </select>
    <p class="registry-picker-hint">${escapeHtml(t('registry_picker.input_hint'))}</p>
  `;
  const select = host.querySelector('select');

  try {
    const inputs = (await loadInputs()).filter(i => i.kind === kind && !i.missing);
    select.innerHTML = `<option value="">${escapeHtml(t('registry_picker.none'))}</option>` +
      inputs.map(i => `
        <option value="${escapeHtml(i.id)}" data-path="${escapeHtml(i.path)}">
          ${escapeHtml(i.display_name)} — ${escapeHtml(formatBytes(i.size_bytes || 0))}
        </option>`).join('');
    if (inputs.length === 0) {
      select.innerHTML += `<option value="" disabled>${escapeHtml(t('registry_picker.input_empty'))}</option>`;
    }
  } catch {
    // keep placeholder
  }

  select.addEventListener('change', () => {
    const id = select.value;
    const opt = select.selectedOptions[0];
    host.dataset.selectedId = id || '';
    const input = id ? inputCache.rows?.find(i => i.id === id) : null;
    if (onPick) {
      onPick(input);
    } else if (targetInput && input?.path) {
      targetInput.value = input.path;
      targetInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
  });

  if (window.lucide) window.lucide.createIcons();
}

/**
 * Attach a Samples multi-picker. When the user picks samples, a callback is
 * invoked with { sample_ids, input_ids, r1_paths, r2_paths, names } so the
 * form can populate its read fields.
 */
export async function attachSamplesPicker(host, onPick) {
  host.innerHTML = `
    <button type="button" class="btn btn-secondary btn-sm registry-picker-btn">
      <i data-lucide="users"></i> ${escapeHtml(t('registry_picker.samples_btn'))}
    </button>
    <p class="registry-picker-hint">${escapeHtml(t('registry_picker.samples_hint'))}</p>
  `;
  const btn = host.querySelector('button');
  btn.addEventListener('click', async () => {
    const [samples, inputs] = await Promise.all([loadSamples(true), loadInputs(true)]);
    if (!samples.length) {
      const { alertModal } = await import('./modal.js');
      alertModal({ title: t('registry_picker.no_samples_title'), message: t('registry_picker.no_samples_message') });
      return;
    }
    const inputMap = new Map(inputs.map(i => [i.id, i]));
    openSamplesModal(samples, inputMap, (pick) => {
      host.dataset.selectedIds = pick.sample_ids.join(',');
      host.dataset.selectedInputIds = pick.input_ids.join(',');
      onPick?.(pick);
    });
  });
  if (window.lucide) window.lucide.createIcons();
}

function openSamplesModal(samples, inputMap, onOk) {
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop';
  const rows = samples.map(s => {
    const files = s.inputs
      .map(id => inputMap.get(id))
      .filter(Boolean)
      .map(i => i.display_name)
      .join(' · ');
    return `
      <label class="samples-pick-row">
        <input type="checkbox" value="${escapeHtml(s.id)}"/>
        <span class="kind-pill ${s.paired ? 'kind-fastq' : 'kind-other'}">${s.paired ? 'PE' : 'SE'}</span>
        <div class="samples-pick-name">
          <strong>${escapeHtml(s.name)}</strong>
          ${s.group ? `<span style="color:var(--text-muted);font-weight:400"> · ${escapeHtml(s.group)}</span>` : ''}
          <div style="font-size:0.72rem;color:var(--text-muted);margin-top:2px">${escapeHtml(files)}</div>
        </div>
      </label>`;
  }).join('');
  backdrop.innerHTML = `
    <div class="modal samples-pick-modal" role="dialog" aria-modal="true">
      <h3 class="modal-title">${escapeHtml(t('registry_picker.samples_modal_title'))}</h3>
      <div class="samples-pick-list">${rows}</div>
      <div class="modal-actions">
        <button type="button" class="btn btn-secondary" data-role="cancel">${escapeHtml(t('common.cancel'))}</button>
        <button type="button" class="btn btn-primary"   data-role="ok">${escapeHtml(t('common.ok'))}</button>
      </div>
    </div>`;
  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();
  backdrop.querySelector('[data-role=cancel]').addEventListener('click', close);
  backdrop.addEventListener('click', e => { if (e.target === backdrop) close(); });
  backdrop.querySelector('[data-role=ok]').addEventListener('click', () => {
    const ids = [...backdrop.querySelectorAll('input:checked')].map(x => x.value);
    const chosen = samples.filter(s => ids.includes(s.id));
    // Produce ordered arrays for the form.
    const r1_paths = [];
    const r2_paths = [];
    const names = [];
    const input_ids = [];
    for (const s of chosen) {
      names.push(s.name);
      const rec1 = inputMap.get(s.inputs[0]);
      const rec2 = s.inputs[1] ? inputMap.get(s.inputs[1]) : null;
      if (rec1) { r1_paths.push(rec1.path); input_ids.push(rec1.id); }
      if (rec2) { r2_paths.push(rec2.path); input_ids.push(rec2.id); }
    }
    onOk({ sample_ids: ids, input_ids, r1_paths, r2_paths, names });
    close();
  });
}

/**
 * Read the lineage arrays from all pickers inside a form.
 * Returns `{ inputsUsed: [], assetsUsed: [] }` ready for modulesApi.run.
 */
export function collectLineage(form) {
  const inputsUsed = new Set();
  const assetsUsed = new Set();
  form.querySelectorAll('.registry-picker').forEach(host => {
    const id = host.dataset.selectedId;
    const ids = host.dataset.selectedInputIds ? host.dataset.selectedInputIds.split(',').filter(Boolean) : [];
    const kind = host.dataset.lineageKey || host.dataset.kind;
    if (id) {
      if (kind === 'asset') assetsUsed.add(id);
      else if (kind === 'input') inputsUsed.add(id);
    }
    ids.forEach(x => inputsUsed.add(x));
  });
  return {
    inputsUsed: [...inputsUsed],
    assetsUsed: [...assetsUsed],
  };
}
