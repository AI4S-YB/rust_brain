// Built-in module set — used as the initial fallback before the backend
// `list_modules` call resolves. After bootstrap, `setBootstrapModules()`
// replaces the contents with the merged first-party + plugin list.
export const MODULES = [
  { id: 'qc',           view_id: 'qc',           name: 'QC Analysis',                    icon: 'microscope', color: 'teal',   tool: 'fastqc-rs',   status: 'ready', backend: 'qc',          source: 'builtin', has_native_view: true,  category: 'qc' },
  { id: 'trimming',     view_id: 'trimming',     name: 'Adapter Trimming',               icon: 'scissors',   color: 'blue',   tool: 'cutadapt-rs', status: 'ready', backend: 'trimming',    source: 'builtin', has_native_view: true,  category: 'trimming' },
  { id: 'star-align',   view_id: 'star-align',   name: 'Alignment & Quantification',     icon: 'git-merge',  color: 'purple', tool: 'STAR_rs',     status: 'ready', backend: 'star_align',  source: 'builtin', has_native_view: true,  category: 'alignment' },
  { id: 'differential', view_id: 'differential', name: 'Differential Expr.',             icon: 'flame',      color: 'coral',  tool: 'DESeq2_rs',   status: 'ready', backend: 'deseq2',      source: 'builtin', has_native_view: true,  category: 'differential' },
  { id: 'network',      view_id: 'network',      name: 'Network Analysis',               icon: 'share-2',    color: 'green',  tool: 'WGCNA_rs',    status: 'soon',  utility: true,                                source: 'builtin', has_native_view: true,  category: 'other' },
  { id: 'enrichment',   view_id: 'enrichment',   name: 'Enrichment',                     icon: 'target',     color: 'slate',  tool: 'TBD',         status: 'soon',                                                source: 'builtin', has_native_view: true,  category: 'other' },
];

/**
 * Replace the contents of MODULES with the dynamic list from `list_modules`,
 * preserving the array reference so other modules' imports stay live.
 * Built-in descriptors get their well-known color; plugins get the 'plug' color.
 */
export function setBootstrapModules(descriptors) {
  MODULES.length = 0;
  for (const d of descriptors) {
    MODULES.push({
      ...d,
      backend: d.id,
      status: 'ready',
      color: d.source === 'builtin' ? colorForBuiltin(d.id) : 'plug',
    });
  }
  rebuildKnownViews();
}

function colorForBuiltin(id) {
  return ({
    qc: 'teal',
    trimming: 'blue',
    star_align: 'purple',
    deseq2: 'coral',
    star_index: 'purple',
    gff_convert: 'gold',
  })[id] || 'slate';
}

export const COLOR_MAP = {
  teal:   '#0d7377',
  blue:   '#3b6ea5',
  purple: '#7c5cbf',
  gold:   '#b8860b',
  coral:  '#c9503c',
  green:  '#2d8659',
  slate:  '#5c7080',
  plug:   '#5c7080',
};

export const KNOWN_VIEWS = new Set();
function rebuildKnownViews() {
  KNOWN_VIEWS.clear();
  ['dashboard', 'settings', 'gff-convert', 'star-index', 'star-align', 'chat', 'plots']
    .forEach(v => KNOWN_VIEWS.add(v));
  MODULES.forEach(m => KNOWN_VIEWS.add(m.view_id || m.id));
}
rebuildKnownViews();

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
