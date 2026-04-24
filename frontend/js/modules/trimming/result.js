import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';

function statusText(status) {
  if (status === 'ok') return t('trimming.status_ok');
  return status || '-';
}

function renderMetric(value, label) {
  return `
    <div class="result-metric">
      <div class="result-metric-value">${escapeHtml(String(value ?? '-'))}</div>
      <div class="result-metric-label">${escapeHtml(label)}</div>
    </div>`;
}

function renderOutputFiles(files) {
  if (!files.length) return `<p><em>${escapeHtml(t('trimming.output_files_empty'))}</em></p>`;
  return `
    <ul class="run-result-list">
      ${files.map(path => `<li class="path" title="${escapeHtml(path)}">${escapeHtml(path)}</li>`).join('')}
    </ul>`;
}

function renderFileRows(files) {
  if (!files.length) return '';
  const rows = files.map(item => `
    <tr>
      <td class="path" title="${escapeHtml(item.file || '')}">${escapeHtml(item.file || '-')}</td>
      <td>${escapeHtml(statusText(item.status))}</td>
      <td class="path" title="${escapeHtml(item.output || '')}">${escapeHtml(item.output || '-')}</td>
    </tr>`).join('');
  return `
    <h3>${escapeHtml(t('trimming.file_status_heading'))}</h3>
    <div class="run-result-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>${escapeHtml(t('trimming.col_input'))}</th>
            <th>${escapeHtml(t('trimming.col_status'))}</th>
            <th>${escapeHtml(t('trimming.col_output'))}</th>
          </tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

function renderLog(log) {
  if (!log) return '';
  return `
    <details class="log-panel">
      <summary>${escapeHtml(t('common.log_panel'))}</summary>
      <pre>${escapeHtml(log)}</pre>
    </details>`;
}

export function renderTrimmingResult(result) {
  const summary = result?.summary || {};
  const files = Array.isArray(summary.files) ? summary.files : [];
  const outputFiles = Array.isArray(result?.output_files) ? result.output_files : [];
  const totalFiles = summary.total_files ?? files.length;
  const trimmedOk = summary.trimmed_ok ?? files.filter(item => item?.status === 'ok').length;

  return `
    <div class="run-result-card trimming-result-card">
      <h3>${escapeHtml(t('trimming.results'))}</h3>
      <div class="results-summary">
        ${renderMetric(totalFiles, t('trimming.metric_total_files'))}
        ${renderMetric(trimmedOk, t('trimming.metric_trimmed_ok'))}
        ${renderMetric(outputFiles.length, t('trimming.metric_output_files'))}
      </div>
      <dl class="result-kv">
        <dt>${escapeHtml(t('trimming.adapter_3'))}</dt><dd>${escapeHtml(summary.adapter || '-')}</dd>
        <dt>${escapeHtml(t('trimming.quality_cutoff'))}</dt><dd>${escapeHtml(String(summary.quality_cutoff ?? '-'))}</dd>
        <dt>${escapeHtml(t('trimming.min_length'))}</dt><dd>${escapeHtml(String(summary.min_length ?? '-'))}</dd>
        <dt>${escapeHtml(t('trimming.output_directory'))}</dt><dd class="path" title="${escapeHtml(summary.output_directory || '')}">${escapeHtml(summary.output_directory || '-')}</dd>
      </dl>
      <h3>${escapeHtml(t('trimming.output_files_label'))}</h3>
      ${renderOutputFiles(outputFiles)}
      ${renderFileRows(files)}
      ${renderLog(result?.log || '')}
    </div>`;
}
