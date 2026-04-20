import { ECHART_THEME, createChart } from '../../core/echarts-theme.js';

export function renderDESeq2Charts() {
  const volcEl = document.getElementById('deseq-volcano-chart');
  const maEl = document.getElementById('deseq-ma-chart');
  const tbody = document.querySelector('#deseq-results-table tbody');

  const n = 2000;
  const genes = [];
  for (let i = 0; i < n; i++) {
    const lfc = (Math.random() - 0.5) * 8;
    const bm = Math.pow(10, 1 + Math.random() * 4);
    const pv = Math.pow(10, -(Math.abs(lfc) * (1 + Math.random() * 3) + Math.random() * 2));
    const pa = Math.min(1, pv * n / (i + 1));
    genes.push({ name: `Gene_${String(i + 1).padStart(5, '0')}`, log2FC: lfc, baseMean: bm, pvalue: pv, padj: pa, nlp: -Math.log10(Math.max(pa, 1e-300)) });
  }

  if (volcEl) {
    const up = genes.filter(g => g.padj < 0.01 && g.log2FC > 1);
    const dn = genes.filter(g => g.padj < 0.01 && g.log2FC < -1);
    const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1);

    const chart = createChart(volcEl);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'Volcano Plot', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: {
        trigger: 'item',
        formatter: p => `${p.data[2]}<br>log2FC: ${p.data[0].toFixed(2)}<br>-log10(padj): ${p.data[1].toFixed(1)}`,
      },
      legend: { data: ['Not Sig.', 'Up', 'Down'], right: 60, top: 10, textStyle: { fontSize: 11 } },
      xAxis: { type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      yAxis: { type: 'value', name: '-log10(padj)', nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      series: [
        {
          name: 'Not Sig.', type: 'scatter', symbolSize: 4,
          data: ns.map(g => [g.log2FC, g.nlp, g.name]),
          itemStyle: { color: 'rgba(168,162,158,0.35)' },
          large: true,
          markLine: {
            silent: true, symbol: 'none',
            lineStyle: { type: 'dashed', color: '#ddd6ca', width: 1 },
            data: [{ xAxis: -1 }, { xAxis: 1 }, { yAxis: 2 }],
          },
        },
        { name: 'Up', type: 'scatter', symbolSize: 5, data: up.map(g => [g.log2FC, g.nlp, g.name]), itemStyle: { color: '#c9503c' }, large: true },
        { name: 'Down', type: 'scatter', symbolSize: 5, data: dn.map(g => [g.log2FC, g.nlp, g.name]), itemStyle: { color: '#3b6ea5' }, large: true },
      ],
    });
    window.addEventListener('resize', () => chart.resize());
  }

  if (maEl) {
    const sig = genes.filter(g => g.padj < 0.01 && Math.abs(g.log2FC) > 1);
    const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1);

    const chart = createChart(maEl);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'MA Plot', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: { trigger: 'item', formatter: p => `log10(Mean): ${p.data[0].toFixed(2)}<br>log2FC: ${p.data[1].toFixed(2)}` },
      legend: { data: ['Not Sig.', 'Significant'], right: 60, top: 10, textStyle: { fontSize: 11 } },
      xAxis: { type: 'value', name: 'log10(Mean Expression)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      yAxis: { type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      series: [
        {
          name: 'Not Sig.', type: 'scatter', symbolSize: 4,
          data: ns.map(g => [Math.log10(g.baseMean), g.log2FC]),
          itemStyle: { color: 'rgba(168,162,158,0.3)' }, large: true,
          markLine: { silent: true, symbol: 'none', lineStyle: { color: '#c8bfb0', width: 1 }, data: [{ yAxis: 0 }] },
        },
        {
          name: 'Significant', type: 'scatter', symbolSize: 5,
          data: sig.map(g => [Math.log10(g.baseMean), g.log2FC]),
          itemStyle: { color: '#c9503c', opacity: 0.6 }, large: true,
        },
      ],
    });
    window.addEventListener('resize', () => chart.resize());
  }

  if (tbody) {
    const sorted = [...genes].sort((a, b) => a.padj - b.padj).slice(0, 30);
    tbody.innerHTML = sorted.map(g => {
      const sc = g.padj < 0.01 && Math.abs(g.log2FC) > 1 ? 'significant' : '';
      const fc = g.log2FC > 0 ? 'positive' : 'negative';
      return `<tr><td class="gene-name">${g.name}</td><td class="${fc}">${g.log2FC.toFixed(3)}</td><td>${g.pvalue.toExponential(2)}</td><td class="${sc}">${g.padj.toExponential(2)}</td></tr>`;
    }).join('');
  }
}
