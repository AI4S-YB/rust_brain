import { ECHART_THEME, createChart } from '../../core/echarts-theme.js';

export function renderWGCNACharts() {
  const modEl = document.getElementById('wgcna-module-chart');
  const traitEl = document.getElementById('wgcna-trait-chart');

  if (modEl) {
    const names = ['turquoise', 'blue', 'brown', 'green', 'yellow', 'red', 'black', 'pink', 'magenta', 'purple', 'greenyellow', 'grey'];
    const sizes = [820, 650, 520, 410, 380, 310, 270, 240, 190, 160, 130, 920];
    const colors = ['#40E0D0', '#4169E1', '#8B6914', '#228B22', '#DAA520', '#DC143C', '#444', '#FF69B4', '#C71585', '#7B68EE', '#7CCD7C', '#999'];

    const chart = createChart(modEl);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'Module Sizes', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: { trigger: 'axis', formatter: p => `${p[0].name}<br>${p[0].value} genes` },
      xAxis: { type: 'category', data: names, name: 'Module', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false }, axisLabel: { rotate: 30 } },
      yAxis: { type: 'value', name: 'Gene Count', nameLocation: 'middle', nameGap: 45, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      series: [{
        type: 'bar',
        data: sizes.map((v, i) => ({ value: v, itemStyle: { color: colors[i] + 'CC' } })),
      }],
    });
    window.addEventListener('resize', () => chart.resize());
  }

  if (traitEl) {
    const mods = ['turquoise', 'blue', 'brown', 'green', 'yellow', 'red'];
    const traits = ['Treatment', 'Time', 'Batch', 'Age'];
    const data = [];
    mods.forEach((m, mi) => {
      traits.forEach((tr, ti) => {
        data.push([ti, mi, parseFloat(((Math.random() - 0.5) * 2).toFixed(2))]);
      });
    });

    const chart = createChart(traitEl);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'Module-Trait Correlation', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: { left: 90, right: 80, top: 50, bottom: 60 },
      toolbox: ECHART_THEME.toolbox,
      tooltip: { formatter: p => `${mods[p.data[1]]} vs ${traits[p.data[0]]}<br>r = ${p.data[2]}` },
      xAxis: { type: 'category', data: traits, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
      yAxis: { type: 'category', data: mods, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
      visualMap: {
        min: -1, max: 1, calculable: true, orient: 'vertical', right: 10, top: 'center',
        inRange: { color: ['#3b6ea5', '#faf8f4', '#c9503c'] },
        textStyle: { color: '#57534e' },
      },
      series: [{
        type: 'heatmap',
        data,
        label: { show: true, formatter: p => p.data[2].toFixed(2), fontSize: 11 },
        emphasis: { itemStyle: { shadowBlur: 10, shadowColor: 'rgba(0,0,0,0.5)' } },
      }],
    });
    window.addEventListener('resize', () => chart.resize());
  }
}
