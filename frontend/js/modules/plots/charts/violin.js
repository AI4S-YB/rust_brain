import {
  baseOption, errorOption, extractGroupsWide, computeBoxStats,
  kdeEstimate, linspace, SERIES_COLORS, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'violin', icon: 'waves', label_key: 'plots.type_violin' };

export function sample() {
  const header = ['Control', 'Low_Dose', 'High_Dose'];
  const rng = mulberry32(99);
  const rows = [];
  for (let i = 0; i < 40; i++) {
    rows.push([
      (5 + gaussian(rng) * 0.8).toFixed(3),
      (6.5 + gaussian(rng) * 1.1).toFixed(3),
      (9 + gaussian(rng) * 1.4).toFixed(3),
    ]);
  }
  return toTsv(header, rows);
}

export function defaults() { return {}; }

export function renderParams(_table, _params, { t }) {
  return `<p class="form-hint">${t('plots.wide_hint')}</p>`;
}

export function build(table) {
  const groups = extractGroupsWide(table);
  if (!groups.length) return errorOption('Violin plot requires numeric columns.');

  const globalMin = Math.min(...groups.map(g => Math.min(...g.values)));
  const globalMax = Math.max(...groups.map(g => Math.max(...g.values)));
  const pad = (globalMax - globalMin) * 0.05 || 1;
  const yMin = globalMin - pad;
  const yMax = globalMax + pad;
  const yGrid = linspace(yMin, yMax, 80);
  const half = 0.42;

  const series = [];
  groups.forEach((g, gi) => {
    const stats = computeBoxStats(g.values);
    const kde = kdeEstimate(g.values, yGrid);
    const maxDensity = Math.max(...kde) || 1;
    const left = kde.map((d, i) => [gi + 1 - (d / maxDensity) * half, yGrid[i]]);
    const right = kde.map((d, i) => [gi + 1 + (d / maxDensity) * half, yGrid[i]]).reverse();
    const color = SERIES_COLORS[gi % SERIES_COLORS.length];

    series.push({
      name: `${g.name} (violin)`,
      type: 'custom',
      renderItem: (_p, api) => {
        const pts = left.concat(right).map(pt => api.coord(pt));
        return { type: 'polygon', shape: { points: pts }, style: { fill: color, opacity: 0.3, stroke: color, lineWidth: 1 } };
      },
      data: [[gi + 1, (yMin + yMax) / 2]],
      clip: true, silent: true,
    });
    series.push({
      name: `${g.name} (box)`,
      type: 'custom',
      renderItem: (_p, api) => {
        const x = gi + 1;
        const halfBox = 0.1;
        const [min, q1, med, q3, max] = stats;
        const xy = (xc, yc) => api.coord([xc, yc]);
        const topMin = xy(x, max), botMax = xy(x, min);
        const q1Pt = xy(x - halfBox, q1), q3Pt = xy(x + halfBox, q3);
        const medL = xy(x - halfBox, med), medR = xy(x + halfBox, med);
        return {
          type: 'group',
          children: [
            { type: 'line', shape: { x1: topMin[0], y1: topMin[1], x2: botMax[0], y2: botMax[1] }, style: { stroke: '#1c1917', lineWidth: 1 } },
            { type: 'rect', shape: { x: q1Pt[0], y: q3Pt[1], width: q3Pt[0] - q1Pt[0], height: q1Pt[1] - q3Pt[1] }, style: { fill: '#faf8f4', stroke: '#1c1917', lineWidth: 1 } },
            { type: 'line', shape: { x1: medL[0], y1: medL[1], x2: medR[0], y2: medR[1] }, style: { stroke: color, lineWidth: 2 } },
          ],
        };
      },
      data: [[gi + 1, (yMin + yMax) / 2]],
      silent: true,
    });
  });

  const opt = baseOption('Violin Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => {
      const gi = Math.round(p.value[0]) - 1;
      const g = groups[gi];
      if (!g) return '';
      const stats = computeBoxStats(g.values);
      return `<b>${g.name}</b><br/>n: ${g.values.length}<br/>median: ${stats[2].toFixed(3)}<br/>IQR: ${stats[1].toFixed(3)}–${stats[3].toFixed(3)}`;
    },
  };
  opt.xAxis = {
    type: 'value', min: 0.4, max: groups.length + 0.6,
    axisLabel: {
      formatter: (v) => {
        const idx = Math.round(v) - 1;
        return idx >= 0 && idx < groups.length ? groups[idx].name : '';
      },
      interval: 0,
    },
    splitNumber: groups.length,
    splitLine: { show: false },
  };
  opt.yAxis = { type: 'value', name: 'Value', min: yMin, max: yMax };
  opt.series = series;
  return opt;
}
