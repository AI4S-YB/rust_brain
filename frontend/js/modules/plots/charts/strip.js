import {
  baseOption, errorOption, pivotLong, computeBoxStats,
  SERIES_COLORS, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'strip', icon: 'more-horizontal', label_key: 'plots.type_strip' };

export function sample() {
  const header = ['group', 'expression'];
  const rng = mulberry32(55);
  const rows = [];
  const groups = [
    { name: 'Wildtype', n: 18, mu: 5.5, sigma: 0.7 },
    { name: 'Knockout', n: 15, mu: 7.2, sigma: 1.1 },
    { name: 'Rescue',   n: 20, mu: 6.0, sigma: 0.9 },
    { name: 'Treated',  n: 24, mu: 8.4, sigma: 1.2 },
  ];
  for (const g of groups) {
    for (let i = 0; i < g.n; i++) {
      rows.push([g.name, (g.mu + gaussian(rng) * g.sigma).toFixed(3)]);
    }
  }
  return toTsv(header, rows);
}

export function defaults() {
  return { groupColumn: 0, valColumn: 1, showBox: true, jitter: 0.25 };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <p class="form-hint">${t('plots.strip_hint')}</p>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_group')}</label>
        <select class="form-select" data-plot-param="groupColumn">${colOptions(params.groupColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_value')}</label>
        <select class="form-select" data-plot-param="valColumn">${colOptions(params.valColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.jitter')}</label>
        <input type="number" class="form-input" data-plot-param="jitter" value="${params.jitter}" step="0.05" min="0" max="0.5"></div>
      <div class="form-group"><label class="form-checkbox">
        <input type="checkbox" data-plot-param="showBox" ${params.showBox ? 'checked' : ''}> ${t('plots.overlay_box')}</label></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const groupIdx = Number(params.groupColumn);
  const valIdx = Number(params.valColumn);
  if ([groupIdx, valIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('Strip plot: pick valid group and value columns.');
  }
  const { groups, cube } = pivotLong(rows, groupIdx, groupIdx, valIdx);
  const perGroup = groups.map(g => ({ name: g, values: cube[g]?.[g] || [] })).filter(g => g.values.length);
  if (!perGroup.length) return errorOption('No numeric values per group.');

  const jitter = Math.max(0, Math.min(0.5, Number(params.jitter ?? 0.25)));
  const showBox = Boolean(params.showBox);

  // deterministic jitter: use index-based sin so identical data looks the same each render
  const jitterAt = (i) => (Math.sin(i * 12.9898 + 78.233) * 43758.5453 % 1);

  const series = [];
  perGroup.forEach((g, gi) => {
    const color = SERIES_COLORS[gi % SERIES_COLORS.length];
    const data = g.values.map((v, i) => {
      const dx = (jitterAt(i + gi * 1000) - 0.5) * 2 * jitter;
      return { value: [gi + dx, v] };
    });
    series.push({
      name: g.name, type: 'scatter', data, symbolSize: 7,
      itemStyle: { color, opacity: 0.78, borderColor: 'rgba(0,0,0,0.15)', borderWidth: 0.5 },
    });
  });

  if (showBox) {
    perGroup.forEach((g, gi) => {
      const color = SERIES_COLORS[gi % SERIES_COLORS.length];
      const stats = computeBoxStats(g.values);
      series.push({
        name: `${g.name} (box)`,
        type: 'custom',
        renderItem: (_p, api) => {
          const cx = gi, half = 0.25;
          const [min, q1, med, q3, max] = stats;
          const topMin = api.coord([cx, max]), botMax = api.coord([cx, min]);
          const q1Pt = api.coord([cx - half, q1]), q3Pt = api.coord([cx + half, q3]);
          const medL = api.coord([cx - half, med]), medR = api.coord([cx + half, med]);
          return {
            type: 'group',
            children: [
              { type: 'line', shape: { x1: topMin[0], y1: topMin[1], x2: botMax[0], y2: botMax[1] }, style: { stroke: color, lineWidth: 1.5 } },
              { type: 'rect', shape: { x: q1Pt[0], y: q3Pt[1], width: q3Pt[0] - q1Pt[0], height: q1Pt[1] - q3Pt[1] }, style: { fill: 'rgba(250,248,244,0.6)', stroke: color, lineWidth: 1.5 } },
              { type: 'line', shape: { x1: medL[0], y1: medL[1], x2: medR[0], y2: medR[1] }, style: { stroke: '#1c1917', lineWidth: 2 } },
            ],
          };
        },
        data: [[gi, 0]],
        silent: true,
        z: 1,
      });
    });
  }

  const opt = baseOption('Strip Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      if (!p.value) return '';
      return `<b>${p.seriesName}</b><br/>value: ${Number(p.value[1]).toFixed(3)}`;
    },
  };
  opt.legend = { top: 12, right: 30, data: perGroup.map(g => g.name) };
  opt.xAxis = {
    type: 'value',
    min: -0.6, max: perGroup.length - 0.4,
    axisLabel: {
      formatter: (v) => {
        const idx = Math.round(v);
        return idx >= 0 && idx < perGroup.length ? perGroup[idx].name : '';
      },
      interval: 0,
    },
    splitLine: { show: false },
  };
  opt.yAxis = { type: 'value', name: header[valIdx], scale: true };
  opt.series = series;
  return opt;
}
