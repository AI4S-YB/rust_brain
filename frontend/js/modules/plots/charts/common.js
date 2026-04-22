// Shared helpers for chart modules.
//
// Per-chart contract (each charts/{id}.js exports):
//   export const meta = { id, icon, label_key };
//   export function sample(): string                   // TSV sample data
//   export function defaults(table): object             // initial params
//   export function renderParams(table, params, h): string   // HTML for params panel
//       where h = { t, colOptions, escapeHtml, guessIdx, detectNumericColumns }
//   export function build(table, params): EChartsOption

import { ECHART_THEME } from '../../../core/constants.js';
import { toNumberOrNaN } from '../data.js';

export const PALETTE = {
  up: '#c9503c',
  down: '#3b6ea5',
  ns: '#b7b1a7',
  accent: '#0d7377',
  warm: '#b8860b',
  cool: '#2d8659',
  purple: '#7c5cbf',
};

export const SERIES_COLORS = [
  PALETTE.accent, PALETTE.up, PALETTE.warm, PALETTE.down, PALETTE.cool, PALETTE.purple,
];

export function baseOption(title) {
  return {
    backgroundColor: ECHART_THEME.backgroundColor,
    textStyle: ECHART_THEME.textStyle,
    animationDuration: 400,
    title: { text: title, left: 20, top: 10, textStyle: ECHART_THEME.title.textStyle },
    grid: { left: 70, right: 30, top: 60, bottom: 60, containLabel: true },
    tooltip: { trigger: 'item', confine: true },
    toolbox: {
      right: 20,
      top: 12,
      feature: {
        dataZoom: { title: { zoom: 'Zoom', back: 'Reset' }, yAxisIndex: 'none' },
        restore: { title: 'Restore' },
      },
    },
  };
}

export function errorOption(message) {
  return {
    backgroundColor: ECHART_THEME.backgroundColor,
    graphic: {
      type: 'text',
      left: 'center', top: 'middle',
      style: { text: message, fill: '#5c7080', font: '14px Karla, sans-serif' },
    },
  };
}

export function thresholdLines(lfcCut, padjCut) {
  return {
    silent: true,
    symbol: 'none',
    lineStyle: { type: 'dashed', color: 'rgba(87, 83, 78, 0.45)' },
    data: [
      [{ xAxis: -lfcCut, yAxis: 0 }, { xAxis: -lfcCut, yAxis: 1e9 }],
      [{ xAxis: lfcCut, yAxis: 0 }, { xAxis: lfcCut, yAxis: 1e9 }],
      [{ xAxis: -1e9, yAxis: -Math.log10(padjCut) }, { xAxis: 1e9, yAxis: -Math.log10(padjCut) }],
    ],
  };
}

// ---------- stats ----------

export function mean(a) {
  return a.reduce((s, v) => s + v, 0) / a.length;
}

export function std(a, m) {
  const mu = m ?? mean(a);
  return Math.sqrt(a.reduce((s, v) => s + (v - mu) ** 2, 0) / a.length);
}

export function variance(a) {
  const mu = mean(a);
  return a.reduce((s, v) => s + (v - mu) ** 2, 0) / a.length;
}

export function computeBoxStats(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  const q = (p) => {
    const idx = (sorted.length - 1) * p;
    const lo = Math.floor(idx), hi = Math.ceil(idx);
    return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
  };
  const q1 = q(0.25), med = q(0.5), q3 = q(0.75);
  const iqr = q3 - q1;
  const lo = q1 - 1.5 * iqr, hi = q3 + 1.5 * iqr;
  const withinMin = sorted.find(v => v >= lo) ?? sorted[0];
  const withinMax = [...sorted].reverse().find(v => v <= hi) ?? sorted[sorted.length - 1];
  return [withinMin, q1, med, q3, withinMax];
}

export function linspace(a, b, n) {
  const step = (b - a) / (n - 1);
  return Array.from({ length: n }, (_, i) => a + i * step);
}

export function kdeEstimate(values, grid) {
  const n = values.length;
  const s = std(values) || 1;
  const h = 1.06 * s * Math.pow(n, -1 / 5); // Silverman's rule
  const bw = Math.max(h, 1e-6);
  return grid.map(x => {
    let sum = 0;
    for (const v of values) {
      const u = (x - v) / bw;
      sum += Math.exp(-0.5 * u * u);
    }
    return sum / (n * bw * Math.sqrt(2 * Math.PI));
  });
}

export function pearson(a, b) {
  const n = Math.min(a.length, b.length);
  if (n < 2) return NaN;
  let sa = 0, sb = 0;
  for (let i = 0; i < n; i++) { sa += a[i]; sb += b[i]; }
  const ma = sa / n, mb = sb / n;
  let num = 0, da = 0, db = 0;
  for (let i = 0; i < n; i++) {
    const x = a[i] - ma, y = b[i] - mb;
    num += x * y; da += x * x; db += y * y;
  }
  const denom = Math.sqrt(da * db);
  return denom === 0 ? 0 : num / denom;
}

// ---------- table shaping ----------

export function extractGroupsWide(table) {
  const { header, rows } = table;
  const out = [];
  header.forEach((name, i) => {
    const values = [];
    for (const r of rows) {
      const v = toNumberOrNaN(r[i]);
      if (Number.isFinite(v)) values.push(v);
    }
    if (values.length >= 3) out.push({ name: String(name || `col_${i + 1}`), values });
  });
  return out;
}

export function pivotLong(rows, catIdx, groupIdx, valIdx) {
  const categories = [];
  const groups = [];
  const cube = {};
  for (const r of rows) {
    const cat = String(r[catIdx] ?? '');
    const grp = String(r[groupIdx] ?? '');
    const val = toNumberOrNaN(r[valIdx]);
    if (!cat || !grp || !Number.isFinite(val)) continue;
    if (!categories.includes(cat)) categories.push(cat);
    if (!groups.includes(grp)) groups.push(grp);
    if (!cube[cat]) cube[cat] = {};
    if (!cube[cat][grp]) cube[cat][grp] = [];
    cube[cat][grp].push(val);
  }
  return { categories, groups, cube };
}

export function sortPaired(labels, values, cmp) {
  const pairs = labels.map((l, i) => [l, values[i]]);
  pairs.sort((a, b) => cmp(a[1], b[1]));
  pairs.forEach((p, i) => { labels[i] = p[0]; values[i] = p[1]; });
}

// ---------- colors ----------

export function hexAlpha(hex, alpha) {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!m) return hex;
  const r = parseInt(m[1], 16), g = parseInt(m[2], 16), b = parseInt(m[3], 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

// ---------- random (for sample generators) ----------

export function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a = (a + 0x6D2B79F5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function gaussian(rng) {
  let u = 0, v = 0;
  while (u === 0) u = rng();
  while (v === 0) v = rng();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

export function toTsv(header, rows) {
  return [header.join('\t'), ...rows.map(r => r.join('\t'))].join('\n');
}

// Returns column indices whose first ≥20 rows are ≥70% numeric — used for default
// column picks (X, Y, value) when the user hasn't selected anything yet.
export function detectNumericColumns(table) {
  if (!table || !table.header?.length || !table.rows?.length) return [];
  const probe = table.rows.slice(0, Math.min(20, table.rows.length));
  const out = [];
  table.header.forEach((_, i) => {
    let numeric = 0, total = 0;
    for (const r of probe) {
      if (i >= r.length) continue;
      total += 1;
      const n = Number(r[i]);
      if (Number.isFinite(n)) numeric += 1;
    }
    if (total > 0 && numeric / total >= 0.7) out.push(i);
  });
  return out;
}
