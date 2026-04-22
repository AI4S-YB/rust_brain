import {
  baseOption, errorOption, pivotLong, kdeEstimate, linspace,
  SERIES_COLORS, hexAlpha, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'ridge', icon: 'mountain-snow', label_key: 'plots.type_ridge' };

export function sample() {
  // long format: each row is (group, value). One density per group.
  const header = ['stage', 'expression'];
  const rng = mulberry32(101);
  const stages = [
    { name: 'G0',   mu: 4.2, sigma: 0.6 },
    { name: 'G1',   mu: 5.1, sigma: 0.7 },
    { name: 'S',    mu: 6.4, sigma: 0.9 },
    { name: 'G2',   mu: 7.2, sigma: 0.8 },
    { name: 'M',    mu: 8.0, sigma: 1.0 },
    { name: 'Post', mu: 5.6, sigma: 1.2 },
  ];
  const rows = [];
  for (const s of stages) {
    for (let i = 0; i < 120; i++) {
      rows.push([s.name, (s.mu + gaussian(rng) * s.sigma).toFixed(3)]);
    }
  }
  return toTsv(header, rows);
}

export function defaults(table) {
  return {
    groupColumn: 0,
    valColumn: 1,
    overlap: 0.7,
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <p class="form-hint">${t('plots.ridge_hint')}</p>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_group')}</label>
        <select class="form-select" data-plot-param="groupColumn">${colOptions(params.groupColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_value')}</label>
        <select class="form-select" data-plot-param="valColumn">${colOptions(params.valColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.ridge_overlap')}</label>
        <input type="number" class="form-input" data-plot-param="overlap" value="${params.overlap}" step="0.1" min="0" max="2">
        <span class="form-hint">${t('plots.ridge_overlap_hint')}</span></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const groupIdx = Number(params.groupColumn);
  const valIdx = Number(params.valColumn);
  if ([groupIdx, valIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('Ridge plot: pick valid group and value columns.');
  }
  const overlap = Math.max(0, Math.min(2, Number(params.overlap ?? 0.7)));

  // reuse pivotLong but treat group as category and invent a synthetic single-cat
  const { groups, cube } = pivotLong(rows, groupIdx, groupIdx, valIdx);
  // cube[g][g] = values per group (since we passed the same idx for cat+group)
  const perGroup = groups.map(g => ({ name: g, values: cube[g]?.[g] || [] })).filter(g => g.values.length >= 3);
  if (!perGroup.length) return errorOption('No numeric values per group.');

  const allValues = perGroup.flatMap(g => g.values);
  const globalMin = Math.min(...allValues);
  const globalMax = Math.max(...allValues);
  const pad = (globalMax - globalMin) * 0.05 || 1;
  const xMin = globalMin - pad;
  const xMax = globalMax + pad;
  const xGrid = linspace(xMin, xMax, 120);

  // Each group is a horizontal ridge at y=i, where density is added to the y position.
  // Normalize densities across groups so they share the same scale.
  const densities = perGroup.map(g => kdeEstimate(g.values, xGrid));
  const maxDensityGlobal = Math.max(...densities.flat()) || 1;
  const heightUnit = 1 + overlap;

  const series = [];
  perGroup.forEach((g, gi) => {
    const color = SERIES_COLORS[gi % SERIES_COLORS.length];
    const baseline = gi * 1;                                  // each ridge sits on integer row
    const top = densities[gi].map((d, i) => [xGrid[i], baseline + (d / maxDensityGlobal) * heightUnit]);
    const bottom = xGrid.map(x => [x, baseline]).reverse();
    const polygon = top.concat(bottom);
    series.push({
      name: g.name,
      type: 'custom',
      renderItem: (_p, api) => {
        const pts = polygon.map(pt => api.coord(pt));
        return { type: 'polygon', shape: { points: pts }, style: { fill: hexAlpha(color, 0.55), stroke: color, lineWidth: 1.2 } };
      },
      data: [[xGrid[0], baseline]],
      silent: true,
      z: perGroup.length - gi,
    });
  });

  // dummy legend markers
  perGroup.forEach((g, gi) => {
    series.push({ name: g.name, type: 'scatter', data: [], itemStyle: { color: SERIES_COLORS[gi % SERIES_COLORS.length] } });
  });

  const opt = baseOption('Ridge Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      const g = perGroup.find(g => g.name === p.seriesName);
      if (!g) return '';
      const n = g.values.length;
      const mean = g.values.reduce((a, b) => a + b, 0) / n;
      return `<b>${g.name}</b><br/>n: ${n}<br/>mean: ${mean.toFixed(3)}`;
    },
  };
  opt.grid = { left: 110, right: 40, top: 60, bottom: 60, containLabel: true };
  opt.xAxis = { type: 'value', name: header[valIdx], nameLocation: 'middle', nameGap: 32, min: xMin, max: xMax };
  opt.yAxis = {
    type: 'value',
    min: -0.2, max: perGroup.length + heightUnit - 0.5,
    axisLabel: {
      formatter: (v) => {
        const idx = Math.round(v);
        return idx >= 0 && idx < perGroup.length ? perGroup[idx].name : '';
      },
      interval: 0,
    },
    splitLine: { show: false },
  };
  opt.series = series;
  return opt;
}
