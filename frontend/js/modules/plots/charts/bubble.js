import { toNumberOrNaN, columnIndex } from '../data.js';
import { baseOption, errorOption, PALETTE, toTsv } from './common.js';

export const meta = { id: 'bubble', icon: 'circle-dot', label_key: 'plots.type_bubble' };

export function sample() {
  const header = ['term', 'gene_ratio', 'count', 'padj'];
  const rows = [
    ['DNA replication',              0.28, 42, 2.1e-11],
    ['Cell cycle',                   0.24, 51, 7.4e-10],
    ['p53 signaling',                0.19, 19, 3.2e-8 ],
    ['Apoptosis',                    0.17, 35, 4.5e-7 ],
    ['MAPK signaling',               0.14, 47, 2.0e-6 ],
    ['PI3K-AKT signaling',           0.12, 44, 6.1e-5 ],
    ['Oxidative phosphorylation',    0.10, 31, 1.9e-4 ],
    ['Wnt signaling',                0.08, 24, 5.6e-4 ],
    ['Hypoxia response',             0.07, 18, 1.1e-3 ],
    ['Glycolysis',                   0.05, 14, 4.3e-3 ],
  ];
  return toTsv(header, rows);
}

export function defaults(table) {
  const header = table?.header || [];
  return {
    labelColumn: 0,
    xColumn: columnIndex(header, ['gene_ratio', 'ratio', 'neg_log10_padj', 'x']) >= 0
      ? columnIndex(header, ['gene_ratio', 'ratio', 'neg_log10_padj', 'x']) : 1,
    sizeColumn: columnIndex(header, ['count', 'gene_count', 'size']) >= 0
      ? columnIndex(header, ['count', 'gene_count', 'size']) : 2,
    colorColumn: columnIndex(header, ['padj', 'pvalue', 'fdr']) >= 0
      ? columnIndex(header, ['padj', 'pvalue', 'fdr']) : '',
    logColor: true,
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_label')}</label>
        <select class="form-select" data-plot-param="labelColumn">${colOptions(params.labelColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_x')}</label>
        <select class="form-select" data-plot-param="xColumn">${colOptions(params.xColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_size')}</label>
        <select class="form-select" data-plot-param="sizeColumn">${colOptions(params.sizeColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_color')}</label>
        <select class="form-select" data-plot-param="colorColumn">${colOptions(params.colorColumn, true)}</select></div>
      <div class="form-group"><label class="form-checkbox">
        <input type="checkbox" data-plot-param="logColor" ${params.logColor ? 'checked' : ''}> ${t('plots.log_color')}</label>
        <span class="form-hint">${t('plots.log_color_hint')}</span></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const labelIdx = Number(params.labelColumn);
  const xIdx = Number(params.xColumn);
  const sizeIdx = Number(params.sizeColumn);
  const colorIdx = params.colorColumn !== '' ? Number(params.colorColumn) : -1;
  const logColor = Boolean(params.logColor);
  if ([labelIdx, xIdx, sizeIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('Bubble plot: pick valid label, X, and size columns.');
  }

  const points = [];
  for (const r of rows) {
    const x = toNumberOrNaN(r[xIdx]);
    const size = toNumberOrNaN(r[sizeIdx]);
    if (!Number.isFinite(x) || !Number.isFinite(size)) continue;
    const label = String(r[labelIdx] ?? '');
    let color = NaN;
    if (colorIdx >= 0) {
      const raw = toNumberOrNaN(r[colorIdx]);
      color = logColor && raw > 0 ? -Math.log10(raw) : raw;
    }
    points.push({ label, x, size, color });
  }
  if (!points.length) return errorOption('No numeric rows found for the bubble plot.');

  points.sort((a, b) => a.x - b.x);
  const labels = points.map(p => p.label);
  const sizes = points.map(p => p.size);
  const minSize = Math.min(...sizes);
  const maxSize = Math.max(...sizes);
  const scaleSize = (v) => {
    if (maxSize === minSize) return 16;
    return 6 + ((v - minSize) / (maxSize - minSize)) * 30;
  };
  const data = points.map((p, i) => ({
    value: [p.x, i, p.size, p.color, p.label],
    symbolSize: scaleSize(p.size),
  }));

  const opt = baseOption('Bubble Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      const [x, , size, color, label] = p.value;
      const colorTxt = Number.isFinite(color)
        ? `<br/>${logColor ? '-log10 ' : ''}${header[colorIdx] || 'color'}: ${Number(color).toFixed(3)}`
        : '';
      return `<b>${label}</b><br/>${header[xIdx]}: ${Number(x).toFixed(3)}<br/>${header[sizeIdx]}: ${size}${colorTxt}`;
    },
  };
  opt.grid = { left: 180, right: 80, top: 60, bottom: 60, containLabel: true };
  opt.xAxis = { type: 'value', name: header[xIdx], nameLocation: 'middle', nameGap: 32, scale: true };
  opt.yAxis = { type: 'category', data: labels, inverse: true, axisLabel: { fontSize: 11 } };
  opt.series = [{
    type: 'scatter', data,
    itemStyle: {
      color: colorIdx >= 0 ? undefined : PALETTE.accent,
      opacity: 0.82,
      borderColor: 'rgba(0,0,0,0.18)', borderWidth: 0.5,
    },
    emphasis: { itemStyle: { borderColor: '#111', borderWidth: 1 } },
  }];

  if (colorIdx >= 0) {
    const colors = points.map(p => p.color).filter(Number.isFinite);
    if (colors.length) {
      opt.visualMap = {
        dimension: 3, min: Math.min(...colors), max: Math.max(...colors),
        calculable: true, orient: 'vertical', right: 10, top: 'middle',
        text: ['Higher', 'Lower'],
        inRange: { color: ['#3b6ea5', '#9db7d1', '#f4d6a7', '#d69b4c', '#c9503c'] },
      };
    }
  }
  return opt;
}
