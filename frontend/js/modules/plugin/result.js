import { escapeHtml } from '../../ui/escape.js';

export function renderPluginResult(result, runId) {
  const summary = result.summary || {};
  const outputDir = summary.output_dir || '';
  const outputs = summary.output_files || result.output_files || [];
  const argv = summary.argv || [];
  const exitCode = summary.exit_code;

  return `
    <div class="plugin-result">
      <div class="result-status-card">
        <div class="status-row"><strong>Status:</strong> Done${exitCode != null ? ` (exit ${exitCode})` : ''}</div>
        ${outputDir ? `<div class="status-row"><strong>Output dir:</strong> <code>${escapeHtml(outputDir)}</code></div>` : ''}
      </div>
      ${renderOutputsList(outputs)}
      ${argv.length ? `<details><summary>Command</summary><pre>${escapeHtml(argv.join(' '))}</pre></details>` : ''}
    </div>
  `;
}

function renderOutputsList(files) {
  if (!files || !files.length) {
    return `<p><em>No output files matched the manifest patterns.</em></p>`;
  }
  const rows = files.map(f => {
    const path = typeof f === 'string' ? f : (f.path || String(f));
    return `<li class="output-file"><code>${escapeHtml(path)}</code></li>`;
  }).join('');
  return `<ul class="output-list">${rows}</ul>`;
}
