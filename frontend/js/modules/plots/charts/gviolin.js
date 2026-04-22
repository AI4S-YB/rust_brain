import {
  baseOption, errorOption, pivotLong, computeBoxStats,
  kdeEstimate, linspace, SERIES_COLORS, hexAlpha, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'gviolin', icon: 'waves', label_key: 'plots.type_gviolin' };

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
      for (let rep = 0; rep < 12; rep++) {
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
    return errorOption('Grouped violin: pick valid category, group, and value columns.');
  }
  const { categories, groups, cube } = pivotLong(rows, catIdx, groupIdx, valIdx);
  if (!categories.length || !groups.length) return errorOption('No group × category combinations found.');

  const allValues = [];
  categories.forEach(c => groups.forEach(g => { (cube[c]?.[g] || []).forEach(v => allValues.push(v)); }));
  const globalMin = Math.min(...allValues);
  const globalMax = Math.max(...allValues);
  const pad = (globalMax - globalMin) * 0.08 || 1;
  const yMin = globalMin - pad;
  const yMax = globalMax + pad;
  const yGrid = linspace(yMin, yMax, 60);

  const numGroups = groups.length;
  const stride = 0.8 / Math.max(1, numGroups);
  const halfWidth = stride * 0.45;

  const series = [];
  groups.forEach((g, gi) => {
    const color = SERIES_COLORS[gi % SERIES_COLORS.length];
    const offset = (gi - (numGroups - 1) / 2) * stride;

    categories.forEach((c, ci) => {
      const values = cube[c]?.[g] || [];
      if (values.length < 3) return;
      const kde = kdeEstimate(values, yGrid);
      const maxD = Math.max(...kde) || 1;
      const cx = ci + 1 + offset;
      const left = kde.map((d, i) => [cx - (d / maxD) * halfWidth, yGrid[i]]);
      const right = kde.map((d, i) => [cx + (d / maxD) * halfWidth, yGrid[i]]).reverse();
      series.push({
        name: `${g}`,
        type: 'custom',
        renderItem: (_p, api) => {
          const pts = left.concat(right).map(pt => api.coord(pt));
          return { type: 'polygon', shape: { points: pts }, style: { fill: hexAlpha(color, 0.32), stroke: color, lineWidth: 1 } };
        },
        data: [[cx, (yMin + yMax) / 2]],
        silent: true,
      });
      const stats = computeBoxStats(values);
      series.push({
        name: `${g} (box)`,
        type: 'custom',
        renderItem: (_p, api) => {
          const boxHalf = halfWidth * 0.28;
          const [min, q1, med, q3, max] = stats;
          const topMin = api.coord([cx, max]), botMax = api.coord([cx, min]);
          const q1Pt = api.coord([cx - boxHalf, q1]), q3Pt = api.coord([cx + boxHalf, q3]);
          const medL = api.coord([cx - boxHalf, med]), medR = api.coord([cx + boxHalf, med]);
          return {
            type: 'group',
            children: [
              { type: 'line', shape: { x1: topMin[0], y1: topMin[1], x2: botMax[0], y2: botMax[1] }, style: { stroke: '#1c1917', lineWidth: 1 } },
              { type: 'rect', shape: { x: q1Pt[0], y: q3Pt[1], width: q3Pt[0] - q1Pt[0], height: q1Pt[1] - q3Pt[1] }, style: { fill: '#faf8f4', stroke: '#1c1917', lineWidth: 1 } },
              { type: 'line', shape: { x1: medL[0], y1: medL[1], x2: medR[0], y2: medR[1] }, style: { stroke: color, lineWidth: 2 } },
            ],
          };
        },
        data: [[cx, (yMin + yMax) / 2]],
        silent: true,
      });
    });
  });

  // legend markers — dummy scatter per group, not drawn (empty data)
  groups.forEach((g, gi) => {
    series.push({ name: g, type: 'scatter', data: [], itemStyle: { color: SERIES_COLORS[gi % SERIES_COLORS.length] } });
  });

  const opt = baseOption('Grouped Violin Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      const gi = groups.findIndex(g => p.seriesName === g);
      const ci = Math.round(p.value[0]) - 1;
      if (gi < 0 || ci < 0 || !categories[ci]) return p.seriesName;
      const values = (cube[categories[ci]]?.[groups[gi]] || []);
      if (!values.length) return '';
      const s = computeBoxStats(values);
      return `<b>${categories[ci]} / ${groups[gi]}</b><br/>n: ${values.length}<br/>median: ${s[2].toFixed(3)}<br/>IQR: ${s[1].toFixed(3)}–${s[3].toFixed(3)}`;
    },
  };
  opt.legend = { top: 12, right: 30, data: groups };
  opt.xAxis = {
    type: 'value', min: 0.4, max: categories.length + 0.6,
    splitLine: { show: false },
    axisLabel: {
      formatter: (v) => {
        const idx = Math.round(v) - 1;
        return idx >= 0 && idx < categories.length ? categories[idx] : '';
      },
      interval: 0,
    },
    splitNumber: categories.length,
  };
  opt.yAxis = { type: 'value', name: header[valIdx], min: yMin, max: yMax, scale: true };
  opt.series = series;
  return opt;
}
