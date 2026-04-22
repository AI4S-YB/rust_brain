// Chart export helpers: PNG (from canvas), SVG (re-render in offscreen svg instance),
// and TSV download for the active data table.

export function exportPng(chart, filename, background = '#faf8f4') {
  if (!chart) return;
  const url = chart.getDataURL({
    type: 'png',
    pixelRatio: 2,
    backgroundColor: background,
  });
  downloadUrl(url, filename.endsWith('.png') ? filename : `${filename}.png`);
}

export function exportSvg(option, filename, { width = 900, height = 600, background = '#faf8f4' } = {}) {
  if (!window.echarts || !option) return;
  const host = document.createElement('div');
  host.style.cssText = `position:absolute;left:-99999px;top:-99999px;width:${width}px;height:${height}px;pointer-events:none;`;
  document.body.appendChild(host);
  const inst = window.echarts.init(host, null, { renderer: 'svg', width, height });
  inst.setOption({ ...option, backgroundColor: background });
  const svg = host.querySelector('svg');
  let serialized = '';
  if (svg) {
    if (!svg.getAttribute('xmlns')) svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    serialized = new XMLSerializer().serializeToString(svg);
  }
  inst.dispose();
  host.remove();
  if (!serialized) return;
  const blob = new Blob([serialized], { type: 'image/svg+xml;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  downloadUrl(url, filename.endsWith('.svg') ? filename : `${filename}.svg`);
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

export function exportTsv(table, filename) {
  if (!table) return;
  const lines = [table.header.join('\t'), ...table.rows.map(r => r.join('\t'))];
  const blob = new Blob([lines.join('\n')], { type: 'text/tab-separated-values;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  downloadUrl(url, filename.endsWith('.tsv') ? filename : `${filename}.tsv`);
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

function downloadUrl(url, filename) {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
}
