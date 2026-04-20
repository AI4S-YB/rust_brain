import { t } from '../../core/i18n-helpers.js';
import { filesApi } from '../../api/files.js';
import { ECHART_THEME } from '../../core/echarts-theme.js';

export function renderStarAlignResult(result, runId) {
  const suffix = runId || 'current';
  const chartId = `star-align-chart-${suffix}`;
  const previewId = `star-align-preview-${suffix}`;
  const btnId = `star-to-deseq-${suffix}`;

  const samples = (result.summary && result.summary.samples) || [];
  const matrixPath = result.summary && result.summary.counts_matrix;
  const data = {
    names: samples.map(s => s.name),
    uniq:  samples.map(s => (s.stats && s.stats.uniquely_mapped) || 0),
    multi: samples.map(s => (s.stats && s.stats.multi_mapped) || 0),
    unmap: samples.map(s => (s.stats && s.stats.unmapped) || 0),
  };
  setTimeout(() => renderMappingRateChart(chartId, data), 0);

  let previewHtml = '';
  if (matrixPath) {
    filesApi.readTablePreview(matrixPath, { maxRows: 50, maxCols: 10 })
      .then(rows => {
        const el = document.getElementById(previewId);
        if (!el || !rows || rows.length === 0) return;
        const header = rows[0].map(c => `<th>${c}</th>`).join('');
        const body = rows.slice(1).map(r => '<tr>' + r.map(c => `<td>${c}</td>`).join('') + '</tr>').join('');
        el.innerHTML = `<table class="preview-table"><thead><tr>${header}</tr></thead><tbody>${body}</tbody></table>`;
      }).catch(() => {});
  } else {
    previewHtml = `<p><em>${t('star_align.no_matrix')}</em></p>`;
  }

  return `
    <h3>${t('star_align.mapping_rate')}</h3>
    <div id="${chartId}" style="width: 100%; height: 320px;"></div>
    <h3>${t('star_align.matrix_preview')}</h3>
    <div id="${previewId}">${t('common.loading')}</div>
    ${matrixPath ? `<button id="${btnId}" data-matrix="${matrixPath}">${t('star_align.use_in_deseq')}</button>` : ''}
    ${previewHtml}
  `;
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
