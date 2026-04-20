export const MODULES = [
  { id: 'qc',             name: 'QC Analysis',          icon: 'microscope',  color: 'teal',   tool: 'fastqc-rs',    status: 'ready',  backend: 'qc' },
  { id: 'trimming',       name: 'Adapter Trimming',     icon: 'scissors',    color: 'blue',   tool: 'cutadapt-rs',  status: 'ready',  backend: 'trimming' },
  { id: 'star-align',     name: 'Alignment & Quantification', icon: 'git-merge', color: 'purple', tool: 'STAR_rs', status: 'ready',  backend: 'star_align' },
  { id: 'differential',   name: 'Differential Expr.',   icon: 'flame',       color: 'coral',  tool: 'DESeq2_rs',    status: 'ready',  backend: 'deseq2' },
  { id: 'network',        name: 'Network Analysis',     icon: 'share-2',     color: 'green',  tool: 'WGCNA_rs',     status: 'soon',   utility: true },
  { id: 'enrichment',     name: 'Enrichment',           icon: 'target',      color: 'slate',  tool: 'TBD',          status: 'soon' },
];

export const COLOR_MAP = {
  teal:   '#0d7377',
  blue:   '#3b6ea5',
  purple: '#7c5cbf',
  gold:   '#b8860b',
  coral:  '#c9503c',
  green:  '#2d8659',
  slate:  '#5c7080',
};

export const KNOWN_VIEWS = new Set([
  'dashboard', 'settings', 'gff-convert', 'star-index', 'star-align', 'chat',
  ...MODULES.map(m => m.id),
]);

export const ECHART_THEME = {
  backgroundColor: '#faf8f4',
  textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
  title: { textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' } },
  grid: { left: 60, right: 24, top: 44, bottom: 50 },
  toolbox: {
    feature: {
      saveAsImage: { title: 'Save PNG', pixelRatio: 2 },
      dataZoom: { title: { zoom: 'Zoom', back: 'Reset' } },
    },
    right: 20, top: 10,
  },
};

export const LOG_BUFFER_MAX = 500;

export const MAX_COMPUTE_LOAD = 8;

export const RUN_TASKS = {
  qc:           { backend: 'qc',          computeCost: 4 },
  trimming:     { backend: 'trimming',    computeCost: 4 },
  differential: { backend: 'deseq2',      computeCost: 2 },
  'star-index': { backend: 'star_index',  computeCost: 6 },
  'star-align': { backend: 'star_align',  computeCost: 7 },
  'gff-convert': { backend: 'gff_convert', computeCost: 1 },
};
