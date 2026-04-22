import {
  baseOption, errorOption, pivotLong, computeBoxStats,
  SERIES_COLORS, hexAlpha, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'gbox', icon: 'boxes', label_key: 'plots.type_gbox' };

export function sample() {
  const header = ['gene', 'group', 'expression'];
  const rng = mulberry32(23);
  const rows = [];
  const genes = ['TP53', 'MYC', 'EGFR', 'BRCA1', 'KRAS', 'VEGFA'];
  const groupShifts = { Ctrl: 0, Treat_A: 0.8, Treat_B: 1.9 };
  for (const gene of genes) {
    const baseline = 4 + rng() * 3;
    const responsive = rng() > 0.3;
    for (const g of Object.keys(groupShifts)) {
      for (let rep = 0; rep < 6; rep++) {
        const shift = responsive ? groupShifts[g] : 0;
        rows.push([gene, g, (baseline + shift + gaussian(rng) * 0.5).toFixed(3)]);
      }
    }
  }
  return toTsv(header, rows);
}

export function defaults() {
  return { catColumn: 0, groupColumn: 1, valColumn: 2 };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <p class="form-hint">${t('plots.long_hint')}</p>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_category')}</label>
        <select class="form-select" data-plot-param="catColumn">${colOptions(params.catColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_group')}</label>
        <select class="form-select" data-plot-param="groupColumn">${colOptions(params.groupColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_value')}</label>
        <select class="form-select" data-plot-param="valColumn">${colOptions(params.valColumn)}</select></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const catIdx = Number(params.catColumn);
  const groupIdx = Number(params.groupColumn);
  const valIdx = Number(params.valColumn);
  if ([catIdx, groupIdx, valIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('Grouped box: pick valid category, group, and value columns.');
  }

  const { categories, groups, cube } = pivotLong(rows, catIdx, groupIdx, valIdx);
  if (!categories.length || !groups.length) return errorOption('No group × category combinations found.');

  const series = [];
  const outliers = { name: 'outliers', type: 'scatter', data: [], symbolSize: 5, itemStyle: { color: '#57534e', opacity: 0.6 } };
  groups.forEach((g, gi) => {
    const color = SERIES_COLORS[gi % SERIES_COLORS.length];
    const boxData = categories.map(c => {
      const values = cube[c]?.[g] || [];
      return values.length ? computeBoxStats(values) : [0, 0, 0, 0, 0];
    });
    series.push({
      name: g, type: 'boxplot', data: boxData,
      itemStyle: { color: hexAlpha(color, 0.35), borderColor: color, borderWidth: 1.4 },
    });
    categories.forEach((c, ci) => {
      const values = cube[c]?.[g] || [];
      if (!values.length) return;
      const [lo, , , , hi] = computeBoxStats(values);
      values.forEach(v => { if (v < lo || v > hi) outliers.data.push([ci, v]); });
    });
  });
  series.push(outliers);

  const opt = baseOption('Grouped Box Plot');
  opt.tooltip = { trigger: 'item', confine: true };
  opt.legend = { top: 12, right: 30, data: groups };
  opt.xAxis = {
    type: 'category', data: categories, boundaryGap: true,
    axisLabel: { rotate: categories.join('').length > 28 ? 30 : 0 },
  };
  opt.yAxis = { type: 'value', name: header[valIdx], scale: true };
  opt.series = series;
  return opt;
}
