import { escapeHtml } from '../../ui/escape.js';

export function renderMissingBinaryCard(manifest) {
  const binaryId = manifest.binary_id;
  const name = manifest.name;
  return `
    <div class="missing-binary-card">
      <div class="missing-binary-icon"><i data-lucide="alert-triangle"></i></div>
      <h2>${escapeHtml(name)} needs a binary path</h2>
      <p>Plugin <code>${escapeHtml(manifest.id)}</code> depends on the binary <code>${escapeHtml(binaryId)}</code>, which is not configured and not on PATH.</p>
      <button type="button" class="btn btn-primary" data-act="goto-settings"><i data-lucide="settings"></i> Open Settings</button>
    </div>
  `;
}
