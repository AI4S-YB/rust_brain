import { t } from '../../core/i18n-helpers.js';
import { filesApi } from '../../api/files.js';
import { escapeHtml } from '../../ui/escape.js';

export function renderCountsMergeResult(result, runId) {
  const suffix = runId || 'current';
  const previewId = `counts-merge-preview-${suffix}`;
  const matrixPath = result.summary && result.summary.counts_matrix;

  if (matrixPath) {
    filesApi.readTablePreview(matrixPath, { maxRows: 50, maxCols: 10, hasHeader: true })
      .then(preview => {
        const el = document.getElementById(previewId);
        if (el) el.innerHTML = renderTablePreview(preview);
      })
      .catch(err => {
        const el = document.getElementById(previewId);
        if (el) el.innerHTML = `<p><em>${escapeHtml(t('counts_merge.preview_failed'))}: ${escapeHtml(String(err))}</em></p>`;
      });
  }

  return `
    <div class="run-result-card">
      <h3>${escapeHtml(t('common.summary'))}</h3>
      <dl class="result-kv">
        <dt>${escapeHtml(t('counts_merge.samples'))}</dt>
        <dd>${escapeHtml(String(result.summary?.sample_count ?? 0))}</dd>
        <dt>${escapeHtml(t('counts_merge.genes'))}</dt>
        <dd>${escapeHtml(String(result.summary?.gene_count ?? 0))}</dd>
        <dt>${escapeHtml(t('counts_merge.strand'))}</dt>
        <dd>${escapeHtml(String(result.summary?.strand || ''))}</dd>
        <dt>${escapeHtml(t('counts_merge.output_matrix'))}</dt>
        <dd class="path" title="${escapeHtml(matrixPath || '')}">${escapeHtml(matrixPath || '')}</dd>
      </dl>
      <h3>${escapeHtml(t('counts_merge.matrix_preview'))}</h3>
      <div id="${previewId}">${matrixPath ? escapeHtml(t('common.loading')) : ''}</div>
      ${matrixPath ? `<button type="button" class="btn btn-primary btn-sm" data-use-in-deseq="${escapeHtml(matrixPath)}">${escapeHtml(t('counts_merge.use_in_deseq'))}</button>` : ''}
    </div>
  `;
}

function renderTablePreview(preview) {
  const headers = Array.isArray(preview?.headers) ? preview.headers : [];
  const rows = Array.isArray(preview?.rows) ? preview.rows : [];
  if (headers.length === 0 && rows.length === 0) {
    return `<p><em>${escapeHtml(t('common.no_summary'))}</em></p>`;
  }
  const head = headers.length
    ? `<thead><tr>${headers.map(c => `<th>${escapeHtml(c)}</th>`).join('')}</tr></thead>`
    : '';
  const body = rows
    .map(r => '<tr>' + r.map(c => `<td>${escapeHtml(c)}</td>`).join('') + '</tr>')
    .join('');
  return `<table class="data-table">${head}<tbody>${body}</tbody></table>`;
}
