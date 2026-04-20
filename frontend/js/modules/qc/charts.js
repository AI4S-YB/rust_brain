import { ECHART_THEME, createChart } from '../../core/echarts-theme.js';

export function renderQCCharts() {
  const el = document.getElementById('qc-quality-chart');
  if (!el) return;

  const pos = Array.from({ length: 150 }, (_, i) => i + 1);
  const mean = pos.map(p => p < 5 ? 32 + Math.random() * 3 : p < 120 ? 34 + Math.random() * 2 : 34 - (p - 120) * 0.15 + Math.random() * 2);
  const lo = mean.map(q => q - 4 - Math.random() * 2);
  const hi = mean.map(q => q + 2 + Math.random());

  const chart = createChart(el);
  chart.setOption({
    backgroundColor: '#faf8f4',
    textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
    title: { text: 'Per Base Sequence Quality', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
    grid: ECHART_THEME.grid,
    toolbox: ECHART_THEME.toolbox,
    tooltip: { trigger: 'axis' },
    xAxis: { type: 'category', data: pos, name: 'Position (bp)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
    yAxis: { type: 'value', name: 'Phred Score', nameLocation: 'middle', nameGap: 40, min: 0, max: 42, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
    visualMap: false,
    series: [
      {
        type: 'line', data: hi, symbol: 'none', lineStyle: { width: 0 }, showSymbol: false,
        areaStyle: { color: 'rgba(13,115,119,0.08)' }, stack: 'band', name: 'hi',
      },
      {
        type: 'line', data: lo, symbol: 'none', lineStyle: { width: 0 }, showSymbol: false,
        areaStyle: { color: 'rgba(13,115,119,0.08)' }, stack: 'band', name: 'lo',
      },
      {
        type: 'line', data: mean, name: 'Mean Quality', symbol: 'none',
        lineStyle: { color: '#0d7377', width: 2.5 }, smooth: false,
        markLine: {
          silent: true, symbol: 'none',
          lineStyle: { type: 'dashed', color: '#ccc', width: 1 },
          data: [{ yAxis: 28 }, { yAxis: 20 }],
        },
      },
    ],
    legend: { show: false },
  });
  window.addEventListener('resize', () => chart.resize());
}
