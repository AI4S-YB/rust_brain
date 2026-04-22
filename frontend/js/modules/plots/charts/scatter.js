import { toNumberOrNaN } from '../data.js';
import { baseOption, errorOption, SERIES_COLORS, mulberry32, toTsv, detectNumericColumns } from './common.js';

export const meta = { id: 'scatter', icon: 'scatter-chart', label_key: 'plots.type_scatter' };

export function sample() {
  const header = ['sample', 'pc1', 'pc2', 'group'];
  const rng = mulberry32(17);
  const rows = [];
  const centers = [[-3, -2, 'A'], [2, 2, 'B'], [0, 4, 'C']];
  for (let i = 0; i < 90; i++) {
    const [cx, cy, g] = centers[i % 3];
    rows.push([`S${i + 1}`, (cx + (rng() - 0.5) * 2).toFixed(3), (cy + (rng() - 0.5) * 2).toFixed(3), g]);
  }
  return toTsv(header, rows);
}

export function defaults(table) {
  const numericCols = detectNumericColumns(table);
  return {
    xColumn: numericCols[0] ?? 0,
    yColumn: numericCols[1] ?? 1,
    groupColumn: '',
    labelColumn: '',
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_x')}</label>
        <select class="form-select" data-plot-param="xColumn">${colOptions(params.xColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_y')}</label>
        <select class="form-select" data-plot-param="yColumn">${colOptions(params.yColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_group')}</label>
        <select class="form-select" data-plot-param="groupColumn">${colOptions(params.groupColumn, true)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_label')}</label>
        <select class="form-select" data-plot-param="labelColumn">${colOptions(params.labelColumn, true)}</select></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const xIdx = Number(params.xColumn);
  const yIdx = Number(params.yColumn);
  const groupIdx = params.groupColumn !== '' ? Number(params.groupColumn) : -1;
  const labelIdx = params.labelColumn !== '' ? Number(params.labelColumn) : -1;
  if (xIdx < 0 || yIdx < 0 || xIdx >= header.length || yIdx >= header.length) {
    return errorOption('Scatter plot: pick valid X and Y columns.');
  }

  const groupMap = new Map();
  for (const r of rows) {
    const x = toNumberOrNaN(r[xIdx]);
    const y = toNumberOrNaN(r[yIdx]);
    if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
    const g = groupIdx >= 0 ? String(r[groupIdx] ?? 'default') : 'points';
    if (!groupMap.has(g)) groupMap.set(g, []);
    const label = labelIdx >= 0 ? String(r[labelIdx] ?? '') : '';
    groupMap.get(g).push({ value: [x, y], name: label });
  }

  const series = [...groupMap.entries()].map(([name, data], i) => ({
    name, type: 'scatter', data, symbolSize: 8,
    itemStyle: { color: SERIES_COLORS[i % SERIES_COLORS.length], opacity: 0.78 },
  }));

  const opt = baseOption('Scatter Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => `${p.name ? `<b>${p.name}</b><br/>` : ''}${header[xIdx]}: ${p.value[0]}<br/>${header[yIdx]}: ${p.value[1]}`,
  };
  opt.legend = { top: 12, right: 30, data: series.map(s => s.name) };
  opt.xAxis = { type: 'value', name: header[xIdx], nameLocation: 'middle', nameGap: 32, scale: true };
  opt.yAxis = { type: 'value', name: header[yIdx], nameLocation: 'middle', nameGap: 50, scale: true };
  opt.series = series;
  return opt;
}

