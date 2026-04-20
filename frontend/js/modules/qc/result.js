import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';

function renderStatusBadge(status) {
  const normalized = String(status || '').toLowerCase();
  if (normalized === 'ok') {
    return `<span class="badge badge-green">${t('qc.status_ok')}</span>`;
  }
  if (normalized === 'error') {
    return `<span class="badge badge-coral">${t('qc.status_error')}</span>`;
  }
  return `<span class="badge badge-muted">${escapeHtml(status || '-')}</span>`;
}

export function renderQcResult(result) {
  const summary = result.summary || {};
  const files = Array.isArray(summary.files) ? summary.files : [];
  const outputFiles = Array.isArray(result.output_files) ? result.output_files : [];
  const outputDir = summary.output_directory || '';
  const log = result.log || '';

  const fileRows = files.length
    ? files.map(item => `
      <tr>
        <td class="path" title="${escapeHtml(item.file || '')}">${escapeHtml(item.file || '')}</td>
        <td>${renderStatusBadge(item.status)}</td>
        <td>${item.error ? escapeHtml(item.error) : '<span class="text-muted">-</span>'}</td>
      </tr>
    `).join('')
    : `<tr><td colspan="3"><em>${t('status.no_runs')}</em></td></tr>`;

  const outputList = outputFiles.length
    ? `<ul class="run-result-list">
        ${outputFiles.map(path => `<li class="path" title="${escapeHtml(path)}">${escapeHtml(path)}</li>`).join('')}
      </ul>`
    : `<p><em>${t('qc.output_files_empty')}</em></p>`;

  return `
    <div class="run-result-card">
      <div class="results-summary">
        <div class="result-metric">
          <div class="result-metric-value">${summary.total_files ?? files.length}</div>
          <div class="result-metric-label">${t('qc.metric_total_files')}</div>
        </div>
        <div class="result-metric">
          <div class="result-metric-value">${summary.processed_ok ?? 0}</div>
          <div class="result-metric-label">${t('qc.metric_processed_ok')}</div>
        </div>
        <div class="result-metric">
          <div class="result-metric-value">${outputFiles.length}</div>
          <div class="result-metric-label">${t('qc.metric_output_files')}</div>
        </div>
      </div>

      <dl class="result-kv">
        <dt>${t('qc.output_directory_label')}</dt>
        <dd class="path" title="${escapeHtml(outputDir)}">${escapeHtml(outputDir)}</dd>
      </dl>

      <h3>${t('qc.files_status_heading')}</h3>
      <table class="data-table">
        <thead>
          <tr>
            <th>${t('qc.col_file')}</th>
            <th>${t('qc.col_status')}</th>
            <th>${t('qc.col_error')}</th>
          </tr>
        </thead>
        <tbody>${fileRows}</tbody>
      </table>

      <h3>${t('qc.output_files_label')}</h3>
      ${outputList}

      <h3>${t('qc.tab_log')}</h3>
      <div class="log-output">${escapeHtml(log)}</div>
    </div>
  `;
}
