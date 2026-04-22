import { toNumberOrNaN } from '../data.js';
import {
  baseOption, errorOption, sortPaired, detectNumericColumns,
  SERIES_COLORS, toTsv,
} from './common.js';

export const meta = { id: 'lollipop', icon: 'candy', label_key: 'plots.type_lollipop' };

export function sample() {
  const header = ['term', 'neg_log10_padj', 'count'];
  const rows = [
    ['DNA replication',              10.7, 42],
    ['Cell cycle',                    9.1, 51],
    ['p53 signaling',                 7.5, 19],
    ['Apoptosis',                     6.3, 35],
    ['MAPK signaling',                5.7, 47],
    ['PI3K-AKT signaling',            4.2, 44],
    ['Oxidative phosphorylation',     3.7, 31],
    ['Wnt signaling',                 3.3, 24],
    ['Hypoxia response',              2.9, 18],
    ['Glycolysis',                    2.4, 14],
  ];
  return toTsv(header, rows);
}

export function defaults(table) {
  const numericCols = detectNumericColumns(table);
  const firstNonNumeric = (table?.header || []).findIndex((_, i) => !numericCols.includes(i));
  return {
    catColumn: firstNonNumeric >= 0 ? firstNonNumeric : 0,
    valColumn: numericCols[0] ?? 1,
    sizeColumn: numericCols[1] ?? '',
    sort: 'desc',
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_cat')}</label>
        <select class="form-select" data-plot-param="catColumn">${colOptions(params.catColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_val')}</label>
        <select class="form-select" data-plot-param="valColumn">${colOptions(params.valColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_size')}</label>
        <select class="form-select" data-plot-param="sizeColumn">${colOptions(params.sizeColumn, true)}</select></div>
    </div>
    <div class="form-group"><label class="form-label">${t('plots.sort')}</label>
      <select class="form-select" data-plot-param="sort">
        <option value="desc" ${params.sort === 'desc' ? 'selected' : ''}>${t('plots.sort_desc')}</option>
        <option value="asc"  ${params.sort === 'asc'  ? 'selected' : ''}>${t('plots.sort_asc')}</option>
        <option value="none" ${params.sort === 'none' ? 'selected' : ''}>${t('plots.sort_none')}</option>
      </select></div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const catIdx = Number(params.catColumn);
  const valIdx = Number(params.valColumn);
  const sizeIdx = params.sizeColumn !== '' ? Number(params.sizeColumn) : -1;
  if ([catIdx, valIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('Lollipop: pick valid category and value columns.');
  }

  const labels = [];
  const values = [];
  const sizes = [];
  for (const r of rows) {
    const v = toNumberOrNaN(r[valIdx]);
    if (!Number.isFinite(v)) continue;
    labels.push(String(r[catIdx] ?? ''));
    values.push(v);
    sizes.push(sizeIdx >= 0 ? toNumberOrNaN(r[sizeIdx]) : NaN);
  }
  if (!labels.length) return errorOption('No numeric rows found for the lollipop plot.');

  // triple-sort: reorder all three arrays in lock-step
  if (params.sort === 'desc' || params.sort === 'asc') {
    const order = labels.map((_, i) => i);
    order.sort((a, b) => params.sort === 'desc' ? values[b] - values[a] : values[a] - values[b]);
    const l2 = order.map(i => labels[i]);
    const v2 = order.map(i => values[i]);
    const s2 = order.map(i => sizes[i]);
    labels.length = 0; values.length = 0; sizes.length = 0;
    labels.push(...l2); values.push(...v2); sizes.push(...s2);
  }

  // horizontal lollipop: render as a line series (the stem from 0 → value)
  // + a scatter series for the dots at the tip, sized by sizeColumn if provided.
  const stemData = labels.map((l, i) => [
    [{ value: [0, i] }, { value: [values[i], i] }],
  ]);

  const validSizes = sizes.filter(Number.isFinite);
  const hasSize = sizeIdx >= 0 && validSizes.length > 0;
  let sMin = 0, sMax = 1;
  if (hasSize) { sMin = Math.min(...validSizes); sMax = Math.max(...validSizes); }
  const scale = (v) => {
    if (!hasSize || !Number.isFinite(v) || sMax === sMin) return 10;
    return 6 + ((v - sMin) / (sMax - sMin)) * 18;
  };

  // reverse arrays for the display since category axis counts bottom-up but we want biggest at top
  const revLabels = labels.slice().reverse();
  const revValues = values.slice().reverse();
  const revSizes = sizes.slice().reverse();

  const opt = baseOption('Lollipop Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      const idx = p.dataIndex;
      const label = p.seriesName === 'tip' ? revLabels[idx] : '';
      const val = revValues[idx];
      const sz = revSizes[idx];
      const szTxt = hasSize && Number.isFinite(sz) ? `<br/>${header[sizeIdx]}: ${sz}` : '';
      return `<b>${label || revLabels[idx]}</b><br/>${header[valIdx]}: ${Number(val).toFixed(3)}${szTxt}`;
    },
  };
  opt.xAxis = { type: 'value', name: header[valIdx], nameLocation: 'middle', nameGap: 32 };
  opt.yAxis = { type: 'category', data: revLabels };
  opt.series = [
    {
      name: 'stem', type: 'custom',
      renderItem: (params2, api) => {
        const y = api.value(1);
        const v = api.value(0);
        const start = api.coord([0, y]);
        const end = api.coord([v, y]);
        return {
          type: 'line',
          shape: { x1: start[0], y1: start[1], x2: end[0], y2: end[1] },
          style: { stroke: SERIES_COLORS[0], lineWidth: 2, opacity: 0.6 },
        };
      },
      data: revValues.map((v, i) => [v, i]),
      z: 1,
      silent: true,
    },
    {
      name: 'tip',
      type: 'scatter',
      data: revValues.map((v, i) => ({
        value: [v, i],
        symbolSize: scale(revSizes[i]),
        itemStyle: { color: SERIES_COLORS[0] },
      })),
      z: 2,
    },
  ];
  return opt;
}
