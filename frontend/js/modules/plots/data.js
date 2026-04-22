// TSV/CSV parser + shared column utilities for the plots module.
// Each chart type colocates its own sample-data factory under charts/{id}.js —
// this file only handles parsing and lookups.

export function parseTable(text) {
  const raw = String(text || '').replace(/\r\n?/g, '\n').trim();
  if (!raw) return { header: [], rows: [], delimiter: '\t' };
  const lines = raw.split('\n').filter(l => l.length > 0 && !l.startsWith('#'));
  if (lines.length === 0) return { header: [], rows: [], delimiter: '\t' };

  const first = lines[0];
  const tabCount = (first.match(/\t/g) || []).length;
  const commaCount = (first.match(/,/g) || []).length;
  const delimiter = tabCount >= commaCount && tabCount > 0
    ? '\t'
    : (commaCount > 0 ? ',' : /\s+/);

  const split = (line) => typeof delimiter === 'string'
    ? line.split(delimiter).map(s => s.trim())
    : line.split(delimiter).map(s => s.trim()).filter(s => s.length > 0);

  const header = split(first);
  const rows = lines.slice(1).map(split);
  return { header, rows, delimiter: typeof delimiter === 'string' ? delimiter : '\\s+' };
}

export function toNumberOrNaN(v) {
  if (v === '' || v == null) return NaN;
  const n = Number(v);
  return Number.isFinite(n) ? n : NaN;
}

export function columnIndex(header, candidates) {
  const lower = header.map(h => String(h || '').toLowerCase());
  for (const cand of candidates) {
    const idx = lower.indexOf(String(cand).toLowerCase());
    if (idx >= 0) return idx;
  }
  return -1;
}
