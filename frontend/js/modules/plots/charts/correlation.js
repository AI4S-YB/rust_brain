import { toNumberOrNaN } from '../data.js';
import { baseOption, errorOption, pearson, mulberry32, gaussian, toTsv } from './common.js';

export const meta = { id: 'correlation', icon: 'layout-grid', label_key: 'plots.type_correlation' };

export function sample() {
  // expression matrix: first column = gene name, rest = samples.
  const samples = ['Ctrl_1', 'Ctrl_2', 'Ctrl_3', 'Treat_1', 'Treat_2', 'Treat_3'];
  const header = ['gene', ...samples];
  const rng = mulberry32(201);
  const rows = [];
  for (let i = 0; i < 60; i++) {
    const baseline = 4 + rng() * 6;
    const deltaByGroup = [0, 0, 0, 1.5 + rng(), 1.5 + rng(), 1.5 + rng()];
    const row = [`Gene_${i + 1}`];
    for (let s = 0; s < samples.length; s++) {
      row.push((baseline + deltaByGroup[s] + gaussian(rng) * 0.3).toFixed(3));
    }
    rows.push(row);
  }
  return toTsv(header, rows);
}

export function defaults() {
  return { axis: 'samples', method: 'pearson' };
}

export function renderParams(_table, params, { t }) {
  return `
    <p class="form-hint">${t('plots.corr_hint')}</p>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.corr_axis')}</label>
        <select class="form-select" data-plot-param="axis">
          <option value="samples" ${params.axis === 'samples' ? 'selected' : ''}>${t('plots.corr_axis_samples')}</option>
          <option value="rows"    ${params.axis === 'rows'    ? 'selected' : ''}>${t('plots.corr_axis_rows')}</option>
        </select></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  if (header.length < 3) return errorOption('Correlation needs a label column + ≥ 2 numeric columns.');

  const axis = params.axis === 'rows' ? 'rows' : 'samples';
  const sampleNames = header.slice(1);

  const rowLabels = [];
  const matrix = [];
  for (const r of rows) {
    const values = sampleNames.map((_, i) => toNumberOrNaN(r[i + 1]));
    if (values.some(v => !Number.isFinite(v))) continue;
    rowLabels.push(String(r[0] ?? ''));
    matrix.push(values);
  }
  if (!matrix.length) return errorOption('No numeric rows found.');

  let labels, vectors;
  if (axis === 'samples') {
    labels = sampleNames;
    vectors = sampleNames.map((_, ci) => matrix.map(row => row[ci]));
  } else {
    // limit row-row matrices to top-50 by variance to keep render fast
    const top = matrix.map((row, i) => ({ i, v: variance(row) })).sort((a, b) => b.v - a.v).slice(0, 50);
    const keepIdx = top.map(x => x.i).sort((a, b) => a - b);
    labels = keepIdx.map(i => rowLabels[i]);
    vectors = keepIdx.map(i => matrix[i]);
  }

  const n = labels.length;
  const flat = [];
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      const r = i === j ? 1 : pearson(vectors[i], vectors[j]);
      flat.push([j, i, Number(r.toFixed(4))]);
    }
  }

  const opt = baseOption('Correlation Heatmap');
  opt.tooltip = {
    position: 'top', confine: true,
    formatter: (p) => `<b>${labels[p.value[1]]}</b> × ${labels[p.value[0]]}<br/>r: ${p.value[2]}`,
  };
  opt.grid = { left: 120, right: 80, top: 60, bottom: 90, containLabel: true };
  opt.xAxis = {
    type: 'category', data: labels, splitArea: { show: true },
    axisLabel: { rotate: 30, fontSize: 11 },
  };
  opt.yAxis = {
    type: 'category', data: labels, splitArea: { show: true },
    axisLabel: { fontSize: 11 }, inverse: true,
  };
  opt.visualMap = {
    min: -1, max: 1,
    calculable: true, orient: 'vertical', right: 10, top: 'middle',
    text: ['+1', '-1'],
    inRange: { color: ['#3b6ea5', '#9db7d1', '#faf8f4', '#e6a896', '#c9503c'] },
  };
  opt.series = [{
    name: 'r', type: 'heatmap', data: flat,
    label: { show: n <= 10, fontSize: 10, formatter: p => p.value[2].toFixed(2) },
    emphasis: { itemStyle: { shadowBlur: 6, shadowColor: 'rgba(0,0,0,0.3)' } },
  }];
  return opt;
}

function variance(a) {
  const n = a.length;
  const mu = a.reduce((s, v) => s + v, 0) / n;
  return a.reduce((s, v) => s + (v - mu) ** 2, 0) / n;
}
