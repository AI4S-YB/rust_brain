import { toNumberOrNaN, columnIndex } from '../data.js';
import {
  baseOption, errorOption, thresholdLines,
  PALETTE, mulberry32, toTsv,
} from './common.js';

export const meta = { id: 'volcano', icon: 'flame', label_key: 'plots.type_volcano' };

export function sample() {
  const header = ['gene', 'log2FoldChange', 'pvalue', 'padj'];
  const rng = mulberry32(42);
  const rows = [];
  for (let i = 0; i < 600; i++) {
    const lfc = (rng() - 0.5) * 10;
    const strength = Math.abs(lfc);
    const p = Math.min(1, Math.max(1e-40, Math.exp(-strength * (1 + rng() * 3)) * (0.5 + rng())));
    const padj = Math.min(1, p * (1 + rng() * 4));
    rows.push([`Gene_${i + 1}`, lfc.toFixed(4), p.toExponential(3), padj.toExponential(3)]);
  }
  return toTsv(header, rows);
}

export function defaults(table) {
  const header = table?.header || [];
  return {
    idColumn: 0,
    lfcColumn: columnIndex(header, ['log2foldchange', 'log2fc', 'logfc']) >= 0
      ? columnIndex(header, ['log2foldchange', 'log2fc', 'logfc']) : 1,
    padjColumn: columnIndex(header, ['padj', 'adj.p.val', 'qvalue', 'fdr', 'pvalue']) >= 0
      ? columnIndex(header, ['padj', 'adj.p.val', 'qvalue', 'fdr', 'pvalue']) : 2,
    lfcCut: 1,
    padjCut: 0.05,
    labelTopN: 10,
  };
}

export function renderParams(table, params, { t, colOptions }) {
  return `
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.col_id')}</label>
        <select class="form-select" data-plot-param="idColumn">${colOptions(params.idColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_log2fc')}</label>
        <select class="form-select" data-plot-param="lfcColumn">${colOptions(params.lfcColumn)}</select></div>
      <div class="form-group"><label class="form-label">${t('plots.col_padj')}</label>
        <select class="form-select" data-plot-param="padjColumn">${colOptions(params.padjColumn)}</select></div>
    </div>
    <div class="form-row">
      <div class="form-group"><label class="form-label">${t('plots.lfc_cut')}</label>
        <input type="number" class="form-input" data-plot-param="lfcCut" value="${params.lfcCut}" step="0.1" min="0"></div>
      <div class="form-group"><label class="form-label">${t('plots.padj_cut')}</label>
        <input type="number" class="form-input" data-plot-param="padjCut" value="${params.padjCut}" step="0.01" min="0" max="1"></div>
      <div class="form-group"><label class="form-label">${t('plots.label_top_n')}</label>
        <input type="number" class="form-input" data-plot-param="labelTopN" value="${params.labelTopN}" step="1" min="0"></div>
    </div>`;
}

export function build(table, params) {
  const { header, rows } = table;
  const idIdx = Number(params.idColumn);
  const lfcIdx = Number(params.lfcColumn);
  const padjIdx = Number(params.padjColumn);
  if (lfcIdx < 0 || padjIdx < 0) {
    return errorOption('Volcano plot requires log2FoldChange and padj columns.');
  }

  const lfcCut = Number(params.lfcCut ?? 1);
  const padjCut = Number(params.padjCut ?? 0.05);
  const labelTopN = Math.max(0, Number(params.labelTopN ?? 10));

  const up = [], down = [], ns = [];
  const scored = [];
  for (const r of rows) {
    const lfc = toNumberOrNaN(r[lfcIdx]);
    const padj = toNumberOrNaN(r[padjIdx]);
    if (!Number.isFinite(lfc) || !Number.isFinite(padj) || padj <= 0) continue;
    const neg = -Math.log10(padj);
    const gene = String(r[idIdx] ?? '').slice(0, 40);
    const point = { value: [lfc, neg], name: gene };
    const sig = padj <= padjCut && Math.abs(lfc) >= lfcCut;
    if (sig && lfc > 0) up.push(point);
    else if (sig && lfc < 0) down.push(point);
    else ns.push(point);
    if (sig) scored.push({ gene, lfc, neg });
  }
  scored.sort((a, b) => (b.neg * Math.abs(b.lfc)) - (a.neg * Math.abs(a.lfc)));
  const labelSet = new Set(scored.slice(0, labelTopN).map(s => s.gene));

  const markSeries = (name, data, color) => ({
    name, type: 'scatter', data,
    symbolSize: 6,
    itemStyle: { color, opacity: 0.78, borderColor: 'rgba(0,0,0,0.12)', borderWidth: 0.5 },
    emphasis: { itemStyle: { borderColor: '#111', borderWidth: 1, shadowBlur: 6 } },
    label: labelSet.size ? {
      show: true,
      formatter: (p) => labelSet.has(p.name) ? p.name : '',
      position: 'top', color: '#1c1917', fontSize: 11,
    } : { show: false },
  });

  const opt = baseOption('Volcano Plot');
  opt.tooltip = {
    trigger: 'item', confine: true,
    formatter: (p) => `<b>${p.name || 'point'}</b><br/>log2FC: ${p.value[0].toFixed(3)}<br/>-log10 padj: ${p.value[1].toFixed(3)}`,
  };
  opt.legend = { top: 12, right: 160, data: ['Up', 'Down', 'NS'] };
  opt.xAxis = { type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 32 };
  opt.yAxis = { type: 'value', name: '-log10 padj', nameLocation: 'middle', nameGap: 50 };
  opt.series = [
    { ...markSeries('NS', ns, PALETTE.ns), markLine: thresholdLines(lfcCut, padjCut) },
    markSeries('Down', down, PALETTE.down),
    markSeries('Up', up, PALETTE.up),
  ];
  return opt;
}
