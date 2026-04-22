import {
  baseOption, errorOption, extractGroupsWide, computeBoxStats,
  PALETTE, mulberry32, gaussian, toTsv,
} from './common.js';

export const meta = { id: 'box', icon: 'box', label_key: 'plots.type_box' };

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
  if (!groups.length) return errorOption('Box plot requires numeric columns.');

  const boxData = groups.map(g => computeBoxStats(g.values));
  const outliers = [];
  groups.forEach((g, i) => {
    const [lo, , , , hi] = boxData[i];
    g.values.forEach(v => { if (v < lo || v > hi) outliers.push([i, v]); });
  });

  const opt = baseOption('Box Plot');
  opt.tooltip = { trigger: 'item', confine: true };
  opt.xAxis = { type: 'category', data: groups.map(g => g.name), boundaryGap: true };
  opt.yAxis = { type: 'value', name: 'Value', scale: true };
  opt.series = [
    {
      name: 'boxplot', type: 'boxplot', data: boxData,
      itemStyle: { color: 'rgba(13, 115, 119, 0.35)', borderColor: PALETTE.accent, borderWidth: 1.5 },
    },
    {
      name: 'outliers', type: 'scatter', data: outliers,
      symbolSize: 6, itemStyle: { color: PALETTE.up, opacity: 0.7 },
    },
  ];
  return opt;
}
