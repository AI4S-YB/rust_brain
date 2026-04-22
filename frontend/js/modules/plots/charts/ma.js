import { toNumberOrNaN, columnIndex } from '../data.js';
import { baseOption, errorOption, PALETTE, mulberry32, toTsv } from './common.js';

export const meta = { id: 'ma', icon: 'activity', label_key: 'plots.type_ma' };

export function sample() {
  // Generate a DESeq2-style table with baseMean as a separate column.
  const header = ['gene', 'baseMean', 'log2FoldChange', 'padj'];
  const rng = mulberry32(71);
  const rows = [];
  for (let i = 0; i < 800; i++) {
    const baseMean = Math.exp(rng() * 10);               // skewed high-dynamic-range
    const lfc = (rng() - 0.5) * 8;
    const strength = Math.abs(lfc) * Math.log10(baseMean + 1) * 0.3;
    const padj = Math.min(1, Math.max(1e-30, Math.exp(-strength) * (0.5 + rng())));
    rows.push([`Gene_${i + 1}`, baseMean.toFixed(2), lfc.toFixed(4), padj.toExponential(3)]);
  }
  return toTsv(header, rows);
}

export function defaults(table) {
  const header = table?.header || [];
  return {
    idColumn: 0,
    meanColumn: columnIndex(header, ['basemean', 'mean', 'a', 'expression']) >= 0
      ? columnIndex(header, ['basemean', 'mean', 'a', 'expression']) : 1,
    lfcColumn: columnIndex(header, ['log2foldchange', 'log2fc', 'logfc', 'm']) >= 0
      ? columnIndex(header, ['log2foldchange', 'log2fc', 'logfc', 'm']) : 2,
    padjColumn: columnIndex(header, ['padj', 'adj.p.val', 'fdr', 'pvalue']) >= 0
      ? columnIndex(header, ['padj', 'adj.p.val', 'fdr', 'pvalue']) : 3,
    padjCut: 0.05,
    lfcCut: 1,
  };
}

export function renderParams(_table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_id')}</label>
        <select class="form-select" data-plot-param="idColumn">${colOptions(params.idColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_base_mean')}</label>
        <select class="form-select" data-plot-param="meanColumn">${colOptions(params.meanColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_log2fc')}</label>
        <select class="form-select" data-plot-param="lfcColumn">${colOptions(params.lfcColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_padj')}</label>
        <select class="form-select" data-plot-param="padjColumn">${colOptions(params.padjColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.padj_cut')}</label>
        <input type="number" class="form-input" data-plot-param="padjCut" value="${params.padjCut}" step="0.01" min="0" max="1"></div>
      <div class="form-group"><label class="form-label">${t('plots.lfc_cut')}</label>
        <input type="number" class="form-input" data-plot-param="lfcCut" value="${params.lfcCut}" step="0.1" min="0"></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const idIdx = Number(params.idColumn);
  const meanIdx = Number(params.meanColumn);
  const lfcIdx = Number(params.lfcColumn);
  const padjIdx = Number(params.padjColumn);
  if ([meanIdx, lfcIdx].some(i => i < 0 || i >= header.length)) {
    return errorOption('MA plot requires baseMean and log2FC columns.');
  }
  const padjCut = Number(params.padjCut ?? 0.05);
  const lfcCut = Number(params.lfcCut ?? 1);

  const up = [], down = [], ns = [];
  for (const r of rows) {
    const m = toNumberOrNaN(r[meanIdx]);
    const lfc = toNumberOrNaN(r[lfcIdx]);
    if (!Number.isFinite(m) || !Number.isFinite(lfc) || m <= 0) continue;
    const padj = padjIdx >= 0 ? toNumberOrNaN(r[padjIdx]) : NaN;
    const sig = Number.isFinite(padj) && padj <= padjCut && Math.abs(lfc) >= lfcCut;
    const gene = String(r[idIdx] ?? '').slice(0, 40);
    const point = { value: [m, lfc], name: gene };
    if (sig && lfc > 0) up.push(point);
    else if (sig && lfc < 0) down.push(point);
    else ns.push(point);
  }

  const mkSeries = (name, data, color, size = 5) => ({
    name, type: 'scatter', data, symbolSize: size,
    itemStyle: { color, opacity: 0.78 },
  });

  const opt = baseOption('MA Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => `<b>${p.name || 'point'}</b><br/>mean: ${Number(p.value[0]).toFixed(2)}<br/>log2FC: ${Number(p.value[1]).toFixed(3)}`,
  };
  opt.legend = { top: 12, right: 160, data: ['Up', 'Down', 'NS'] };
  opt.xAxis = {
    type: 'log', logBase: 10,
    name: 'Mean expression (log scale)',
    nameLocation: 'middle', nameGap: 32,
  };
  opt.yAxis = {
    type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 50, scale: true,
  };
  opt.series = [
    {
      ...mkSeries('NS', ns, PALETTE.ns, 4),
      markLine: {
        silent: true, symbol: 'none',
        lineStyle: { type: 'dashed', color: 'rgba(87, 83, 78, 0.45)' },
        data: [
          { yAxis: 0, lineStyle: { color: 'rgba(87, 83, 78, 0.75)', type: 'solid' } },
          { yAxis: lfcCut },
          { yAxis: -lfcCut },
        ],
      },
    },
    mkSeries('Down', down, PALETTE.down),
    mkSeries('Up', up, PALETTE.up),
  ];
  return opt;
}
