import { t } from '../../core/i18n-helpers.js';
import { filesApi } from '../../api/files.js';
import { ECHART_THEME } from '../../core/echarts-theme.js';
import { escapeHtml } from '../../ui/escape.js';

export function renderStarAlignResult(result, runId) {
  const suffix = runId || 'current';
  const chartId = `star-align-chart-${suffix}`;
  const previewId = `star-align-preview-${suffix}`;

  const samples = (result.summary && result.summary.samples) || [];
  const firstSample = samples.find(s => s && s.reads_per_gene) || {};
  const readsPerGenePath = (result.summary && result.summary.reads_per_gene) || firstSample.reads_per_gene;
  const data = {
    names: samples.map(s => s.name),
    uniq:  samples.map(s => (s.stats && s.stats.uniquely_mapped) || 0),
    multi: samples.map(s => (s.stats && s.stats.multi_mapped) || 0),
    unmap: samples.map(s => (s.stats && s.stats.unmapped) || 0),
  };
  setTimeout(() => renderMappingRateChart(chartId, data), 0);

  let previewHtml = '';
  if (readsPerGenePath) {
    filesApi.readTablePreview(readsPerGenePath, { maxRows: 50, maxCols: 4, hasHeader: false })
      .then(preview => {
        const el = document.getElementById(previewId);
        if (!el) return;
        el.innerHTML = renderTablePreview(preview);
      }).catch(err => {
        const el = document.getElementById(previewId);
        if (el) el.innerHTML = `<p><em>${escapeHtml(t('star_align.preview_failed'))}: ${escapeHtml(String(err))}</em></p>`;
      });
  } else {
    previewHtml = `<p><em>${t('star_align.no_reads_per_gene')}</em></p>`;
  }

  return `
    <h3>${t('star_align.mapping_rate')}</h3>
    <div id="${chartId}" style="width: 100%; height: 320px;"></div>
    <h3>${t('star_align.reads_per_gene_preview')}</h3>
    <div id="${previewId}">${readsPerGenePath ? t('common.loading') : ''}</div>
    ${previewHtml}
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

export function renderMappingRateChart(elId, data) {
  const el = document.getElementById(elId);
  if (!el || !window.echarts) return;
  const chart = window.echarts.init(el, ECHART_THEME);
  chart.setOption({
    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
    legend: { data: ['Unique', 'Multi', 'Unmapped'] },
    grid: { left: 60, right: 20, top: 40, bottom: 50 },
    xAxis: { type: 'category', data: data.names },
    yAxis: { type: 'value', name: 'Reads' },
    series: [
      { name: 'Unique',   type: 'bar', stack: 'total', data: data.uniq },
      { name: 'Multi',    type: 'bar', stack: 'total', data: data.multi },
      { name: 'Unmapped', type: 'bar', stack: 'total', data: data.unmap },
    ],
  });
}
