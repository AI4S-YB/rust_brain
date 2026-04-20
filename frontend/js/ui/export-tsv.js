export function exportTableAsTSV(tableId, filename) {
  const table = document.getElementById(tableId);
  if (!table) return;
  const rows = Array.from(table.querySelectorAll('tr'));
  const tsv = rows.map(row =>
    Array.from(row.querySelectorAll('th, td')).map(cell => cell.textContent.trim()).join('\t')
  ).join('\n');
  const blob = new Blob([tsv], { type: 'text/tab-separated-values' });
  const a = document.createElement('a'); a.href = URL.createObjectURL(blob);
  a.download = filename || 'export.tsv'; a.click(); URL.revokeObjectURL(a.href);
}
