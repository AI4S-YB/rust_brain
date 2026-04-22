import { toNumberOrNaN } from '../data.js';
import { baseOption, errorOption, PALETTE, sortPaired, toTsv, detectNumericColumns } from './common.js';

export const meta = { id: 'bar', icon: 'bar-chart-3', label_key: 'plots.type_bar' };

export function sample() {
  const header = ['pathway', 'gene_count', 'neg_log10_padj'];
  const rows = [
    ['Cell cycle', 42, 18.2],
    ['DNA replication', 28, 14.8],
    ['Apoptosis', 35, 11.6],
    ['p53 signaling', 19, 9.1],
    ['MAPK signaling', 51, 7.3],
    ['PI3K-AKT signaling', 47, 6.5],
    ['Wnt signaling', 24, 4.9],
    ['Oxidative phosphorylation', 31, 3.8],
  ];
  return toTsv(header, rows);
}

export function defaults(table) {
  const numericCols = detectNumericColumns(table);
  const firstNonNumeric = (table?.header || []).findIndex((_, i) => !numericCols.includes(i));
  return {
    catColumn: firstNonNumeric >= 0 ? firstNonNumeric : 0,
    valColumn: numericCols[0] ?? 1,
    sort: 'desc',
    horizontal: true,
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_cat')}</label>
        <select class="form-select" data-plot-param="catColumn">${colOptions(params.catColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_val')}</label>
        <select class="form-select" data-plot-param="valColumn">${colOptions(params.valColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.sort')}</label>
        <select class="form-select" data-plot-param="sort">
          <option value="desc" ${params.sort === 'desc' ? 'selected' : ''}>${t('plots.sort_desc')}</option>
          <option value="asc"  ${params.sort === 'asc'  ? 'selected' : ''}>${t('plots.sort_asc')}</option>
          <option value="none" ${params.sort === 'none' ? 'selected' : ''}>${t('plots.sort_none')}</option>
        </select></div>
      <div class="form-group"><label class="form-checkbox">
        <input type="checkbox" data-plot-param="horizontal" ${params.horizontal ? 'checked' : ''}> ${t('plots.horizontal')}</label></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const catIdx = Number(params.catColumn);
  const valIdx = Number(params.valColumn);
  if (catIdx >= header.length || valIdx >= header.length) {
    return errorOption('Bar plot: pick valid category and value columns.');
  }
  const horizontal = Boolean(params.horizontal);

  const labels = [];
  const values = [];
  for (const r of rows) {
    const v = toNumberOrNaN(r[valIdx]);
    if (!Number.isFinite(v)) continue;
    labels.push(String(r[catIdx] ?? ''));
    values.push(v);
  }
  if (params.sort === 'desc') sortPaired(labels, values, (a, b) => b - a);
  else if (params.sort === 'asc') sortPaired(labels, values, (a, b) => a - b);

  const opt = baseOption('Bar Plot');
  opt.tooltip = { trigger: 'axis', axisPointer: { type: 'shadow' } };
  if (horizontal) {
    opt.xAxis = { type: 'value', name: header[valIdx], nameLocation: 'middle', nameGap: 32 };
    opt.yAxis = { type: 'category', data: labels.slice().reverse() };
    opt.series = [{ type: 'bar', data: values.slice().reverse(), itemStyle: { color: PALETTE.accent }, barMaxWidth: 28 }];
  } else {
    opt.xAxis = { type: 'category', data: labels, axisLabel: { rotate: 30 } };
    opt.yAxis = { type: 'value', name: header[valIdx], nameLocation: 'middle', nameGap: 50 };
    opt.series = [{ type: 'bar', data: values, itemStyle: { color: PALETTE.accent }, barMaxWidth: 40 }];
  }
  return opt;
}

