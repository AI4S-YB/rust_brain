import { toNumberOrNaN } from '../data.js';
import { baseOption, errorOption, mean, std, variance, mulberry32, gaussian, toTsv } from './common.js';

export const meta = { id: 'heatmap', icon: 'grid-3x3', label_key: 'plots.type_heatmap' };

export function sample() {
  const samples = ['Ctrl_1', 'Ctrl_2', 'Ctrl_3', 'Treat_1', 'Treat_2', 'Treat_3'];
  const header = ['gene', ...samples];
  const rng = mulberry32(7);
  const geneNames = ['TP53', 'BRCA1', 'MYC', 'EGFR', 'KRAS', 'VEGFA', 'PTEN', 'AKT1', 'MTOR', 'IL6', 'TNF', 'CDK2', 'CDK4', 'CCND1', 'RB1', 'BCL2', 'BAX', 'CASP3', 'FOXO1', 'STAT3'];
  const rows = [];
  for (const g of geneNames) {
    const baseline = 3 + rng() * 5;
    const row = [g];
    const isUp = rng() > 0.5;
    for (let i = 0; i < 6; i++) {
      const treat = i >= 3;
      const delta = treat ? (isUp ? 1 + rng() * 2 : -1 - rng() * 2) : 0;
      row.push((baseline + delta + (rng() - 0.5) * 0.3).toFixed(3));
    }
    rows.push(row);
  }
  return toTsv(header, rows);
}

export function defaults() {
  return { zscore: true, topN: 0 };
}

export function renderParams(table, params, { t }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-checkbox">
        <input type="checkbox" data-plot-param="zscore" ${params.zscore ? 'checked' : ''}> ${t('plots.zscore_rows')}</label>
        <span class="form-hint">${t('plots.zscore_hint')}</span></div>
      <div class="form-group"><label class="form-label">${t('plots.top_n_var')}</label>
        <input type="number" class="form-input" data-plot-param="topN" value="${params.topN}" step="10" min="0">
        <span class="form-hint">${t('plots.top_n_hint')}</span></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  if (header.length < 2) return errorOption('Heatmap needs at least one data column.');

  const zscore = Boolean(params.zscore);
  const topN = Number(params.topN || 0);
  const rowLabels = [];
  const colLabels = header.slice(1);
  const matrix = [];
  for (const r of rows) {
    if (!r.length) continue;
    const label = String(r[0] ?? '');
    const values = colLabels.map((_, i) => toNumberOrNaN(r[i + 1]));
    if (values.some(v => !Number.isFinite(v))) continue;
    rowLabels.push(label);
    matrix.push(values);
  }
  if (!matrix.length) return errorOption('No numeric rows found for the heatmap.');

  let displayMatrix = matrix;
  let displayLabels = rowLabels;
  if (topN > 0 && matrix.length > topN) {
    const variances = matrix.map((row, i) => ({ i, v: variance(row) }));
    variances.sort((a, b) => b.v - a.v);
    const keep = variances.slice(0, topN).map(x => x.i).sort((a, b) => a - b);
    displayMatrix = keep.map(i => matrix[i]);
    displayLabels = keep.map(i => rowLabels[i]);
  }

  let processed = displayMatrix;
  if (zscore) {
    processed = displayMatrix.map(row => {
      const m = mean(row);
      const s = std(row, m) || 1;
      return row.map(v => (v - m) / s);
    });
  }

  const flat = [];
  let minV = Infinity, maxV = -Infinity;
  processed.forEach((row, ri) => {
    row.forEach((v, ci) => {
      flat.push([ci, ri, Number(v.toFixed(4))]);
      if (v < minV) minV = v;
      if (v > maxV) maxV = v;
    });
  });
  const absMax = Math.max(Math.abs(minV), Math.abs(maxV)) || 1;
  const symmetric = zscore || (minV < 0 && maxV > 0);

  const opt = baseOption('Heatmap');
  opt.tooltip = {
    position: 'top', confine: true,
    formatter: (p) => `<b>${displayLabels[p.value[1]]}</b> × ${colLabels[p.value[0]]}<br/>value: ${p.value[2]}`,
  };
  opt.grid = { left: 100, right: 80, top: 60, bottom: 90, containLabel: true };
  opt.xAxis = {
    type: 'category', data: colLabels, splitArea: { show: true },
    axisLabel: { rotate: 30, fontSize: 11 },
  };
  opt.yAxis = {
    type: 'category', data: displayLabels, splitArea: { show: true },
    axisLabel: { fontSize: 11 }, inverse: true,
  };
  opt.visualMap = {
    min: symmetric ? -absMax : minV,
    max: symmetric ? absMax : maxV,
    calculable: true, orient: 'vertical', right: 10, top: 'middle',
    text: ['High', 'Low'],
    inRange: {
      color: symmetric
        ? ['#3b6ea5', '#9db7d1', '#faf8f4', '#e6a896', '#c9503c']
        : ['#faf8f4', '#f4d6a7', '#d69b4c', '#b8860b', '#7a4e07'],
    },
  };
  opt.series = [{
    name: 'value', type: 'heatmap', data: flat,
    label: { show: processed.length * colLabels.length <= 120, fontSize: 10 },
    emphasis: { itemStyle: { shadowBlur: 6, shadowColor: 'rgba(0,0,0,0.3)' } },
    progressive: 1000,
  }];
  return opt;
}
