import { ECHART_THEME, createChart } from '../../core/echarts-theme.js';

export function renderTrimmingCharts() {
  const el = document.getElementById('trim-length-chart');
  if (!el) return;

  const lens = Array.from({ length: 131 }, (_, i) => i + 20);
  const counts = lens.map(l => Math.floor(80000 * Math.exp(-0.5 * ((l - 148) / 8) ** 2) + Math.random() * 1000));
  const colors = lens.map(l => l < 50 ? 'rgba(184,134,11,0.7)' : 'rgba(59,110,165,0.6)');

  const chart = createChart(el);
  chart.setOption({
    backgroundColor: '#faf8f4',
    textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
    title: { text: 'Read Length Distribution After Trimming', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
    grid: ECHART_THEME.grid,
    toolbox: ECHART_THEME.toolbox,
    tooltip: { trigger: 'axis', formatter: params => `Length: ${params[0].name} bp<br>Count: ${params[0].value.toLocaleString()}` },
    xAxis: { type: 'category', data: lens.map(String), name: 'Read Length (bp)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
    yAxis: { type: 'value', name: 'Count', nameLocation: 'middle', nameGap: 50, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
    series: [{
      type: 'bar', data: counts.map((v, i) => ({ value: v, itemStyle: { color: colors[i] } })),
      barWidth: '95%',
    }],
  });
  window.addEventListener('resize', () => chart.resize());
}
