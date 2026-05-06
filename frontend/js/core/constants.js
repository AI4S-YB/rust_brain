// Built-in module set — used as the initial fallback before the backend
// `list_modules` call resolves. After bootstrap, `setBootstrapModules()`
// replaces the contents with the merged first-party + plugin list.
export const MODULES = [
  { id: 'qc',           view_id: 'qc',           name: 'QC Analysis',                    icon: 'microscope', color: 'teal',   tool: 'fastqc-rs',   status: 'ready', backend: 'qc',          source: 'builtin', has_native_view: true,  category: 'qc' },
  { id: 'trimming',     view_id: 'trimming',     name: 'Adapter Trimming',               icon: 'scissors',   color: 'blue',   tool: 'cutadapt-rs', status: 'ready', backend: 'trimming',    source: 'builtin', has_native_view: true,  category: 'trimming' },
  { id: 'star-align',   view_id: 'star-align',   name: 'STAR Single-Sample Alignment',   icon: 'git-merge',  color: 'purple', tool: 'STAR_rs',     status: 'ready', backend: 'star_align',  source: 'builtin', has_native_view: true,  category: 'alignment' },
  { id: 'counts-merge', view_id: 'counts-merge', name: 'Counts Matrix Merge',            icon: 'table',      color: 'green',  tool: 'STAR ReadsPerGene', status: 'ready', backend: 'counts_merge', source: 'builtin', has_native_view: true, category: 'quantification' },
  { id: 'rustqc',       view_id: 'rustqc',       name: 'RNA-Seq Post-Align QC',          icon: 'shield-check', color: 'teal', tool: 'RustQC',      status: 'ready', backend: 'rustqc',      source: 'builtin', has_native_view: true,  category: 'qc' },
  { id: 'differential', view_id: 'differential', name: 'Differential Expr.',             icon: 'flame',      color: 'coral',  tool: 'DESeq2_rs',   status: 'ready', backend: 'deseq2',      source: 'builtin', has_native_view: true,  category: 'differential' },
  { id: 'gene-length',  view_id: 'gene-length',  name: 'Gene Length',                    icon: 'ruler',      color: 'gold',   tool: 'GTF Parser',  status: 'ready', backend: 'gene_length', source: 'builtin', has_native_view: true,  category: 'utility' },
  { id: 'expr-norm',    view_id: 'expr-norm',    name: 'Expression Normalize',           icon: 'sigma',      color: 'green',  tool: 'TPM / FPKM',  status: 'ready', backend: 'expr_norm',   source: 'builtin', has_native_view: true,  category: 'utility' },
  { id: 'network',      view_id: 'network',      name: 'Network Analysis',               icon: 'share-2',    color: 'green',  tool: 'WGCNA_rs',    status: 'soon',  utility: true,                                source: 'builtin', has_native_view: true,  category: 'other' },
  { id: 'enrichment',   view_id: 'enrichment',   name: 'Enrichment',                     icon: 'target',     color: 'slate',  tool: 'TBD',         status: 'soon',                                                source: 'builtin', has_native_view: true,  category: 'other' },
];

// Frontend-only "coming soon" placeholders. They have a sidebar entry and a
// view but no backend Module impl yet, so `list_modules` won't return them.
// Preserved across bootstrap so navigating to them still renders a Coming Soon
// page instead of "Module not found".
const COMING_SOON_MODULES = MODULES.filter(m => m.status === 'soon').map(m => ({ ...m }));

// Built-in utility set — used as the initial fallback before future bootstrap
// calls. Utilities are presentation-only tools with no Module semantics.
export const UTILITIES = [
  { id: 'genome-viewer', view_id: 'genome-viewer', name: 'Genome Viewer', icon: 'map',       color: 'purple', category: 'viewer', source: 'builtin' },
  { id: 'fastq-viewer',  view_id: 'fastq-viewer',  name: 'FASTQ Viewer',  icon: 'file-text', color: 'teal',   category: 'viewer', source: 'builtin' },
  { id: 'bam-tools',     view_id: 'bam-tools',     name: 'BAM Tools',     icon: 'database',  color: 'coral',  category: 'tool',   source: 'builtin' },
];

/**
 * Replace the contents of UTILITIES with the dynamic list, preserving the array
 * reference so other modules' imports stay live.
 */
export function setBootstrapUtilities(descriptors) {
  UTILITIES.length = 0;
  for (const d of descriptors) UTILITIES.push({ ...d, source: d.source || 'builtin' });
  rebuildKnownViews();
}

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
  for (const m of COMING_SOON_MODULES) {
    if (!MODULES.some(x => x.id === m.id || x.view_id === m.view_id)) {
      MODULES.push({ ...m });
    }
  }
  rebuildKnownViews();
}

function colorForBuiltin(id) {
  return ({
    qc: 'teal',
    rustqc: 'teal',
    trimming: 'blue',
    star_align: 'purple',
    counts_merge: 'green',
    deseq2: 'coral',
    star_index: 'purple',
    gff_convert: 'gold',
    gene_length: 'gold',
    expr_norm: 'green',
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
  ['dashboard', 'settings', 'gff-convert', 'star-index', 'star-align', 'counts-merge', 'chat', 'agent', 'plots', 'tasks', 'inputs', 'samples', 'assets', 'gene-length', 'expr-norm']
    .forEach(v => KNOWN_VIEWS.add(v));
  MODULES.forEach(m => KNOWN_VIEWS.add(m.view_id || m.id));
  UTILITIES.forEach(u => KNOWN_VIEWS.add(u.view_id || u.id));
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
  rustqc:       { backend: 'rustqc',      computeCost: 4 },
  trimming:     { backend: 'trimming',    computeCost: 4 },
  differential: { backend: 'deseq2',      computeCost: 2 },
  'star-index': { backend: 'star_index',  computeCost: 6 },
  'star-align': { backend: 'star_align',  computeCost: 7 },
  'counts-merge': { backend: 'counts_merge', computeCost: 1 },
  'gff-convert': { backend: 'gff_convert', computeCost: 1 },
  'gene-length': { backend: 'gene_length', computeCost: 1 },
  'expr-norm':   { backend: 'expr_norm',   computeCost: 1 },
};
