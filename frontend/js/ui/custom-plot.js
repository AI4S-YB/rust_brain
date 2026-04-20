import { ECHART_THEME, createChart } from '../core/echarts-theme.js';

export function renderCustomPlotPanel(moduleId) {
  const axisOptions = moduleId === 'differential'
    ? ['log2FC', 'baseMean', '-log10(padj)', 'pvalue', 'padj']
    : ['module_size', 'kME', 'connectivity', 'trait_correlation'];
  const opts = axisOptions.map(o => `<option value="${o}">${o}</option>`).join('');
  return `
    <div style="padding:12px 0;">
      <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:flex-end;margin-bottom:12px;">
        <div class="form-group" style="margin-bottom:0;min-width:120px;">
          <label class="form-label">X Axis</label>
          <select class="form-select" id="${moduleId}-custom-x">${opts}</select>
        </div>
        <div class="form-group" style="margin-bottom:0;min-width:120px;">
          <label class="form-label">Y Axis</label>
          <select class="form-select" id="${moduleId}-custom-y">${opts.replace('selected', '').replace(axisOptions[0], axisOptions[Math.min(1, axisOptions.length - 1)])}</select>
        </div>
        <div class="form-group" style="margin-bottom:0;min-width:110px;">
          <label class="form-label">Chart Type</label>
          <select class="form-select" id="${moduleId}-custom-type">
            <option value="scatter">Scatter</option>
            <option value="bar">Bar</option>
            <option value="boxplot">Boxplot</option>
            <option value="histogram">Histogram</option>
          </select>
        </div>
        <button class="btn btn-primary btn-sm" onclick="renderCustomPlot('${moduleId}')"><i data-lucide="bar-chart-2"></i> Draw</button>
      </div>
      <div class="chart-container" id="${moduleId}-custom-chart" style="height:320px;"></div>
    </div>`;
}

export function renderCustomPlot(moduleId) {
  const echarts = window.echarts;
  const el = document.getElementById(`${moduleId}-custom-chart`);
  if (!el) return;
  const xSel = document.getElementById(`${moduleId}-custom-x`);
  const ySel = document.getElementById(`${moduleId}-custom-y`);
  const typeSel = document.getElementById(`${moduleId}-custom-type`);
  const xKey = xSel ? xSel.value : 'X';
  const yKey = ySel ? ySel.value : 'Y';
  const chartType = typeSel ? typeSel.value : 'scatter';

  const n = 80;
  const xData = Array.from({ length: n }, () => (Math.random() - 0.5) * 8);
  const yData = xData.map(x => x * 0.4 + (Math.random() - 0.5) * 4);

  const existingChart = echarts.getInstanceByDom(el);
  if (existingChart) existingChart.dispose();
  const chart = createChart(el);

  let series;
  if (chartType === 'scatter') {
    series = [{
      type: 'scatter',
      data: xData.map((x, i) => [x, yData[i]]),
      symbolSize: 6,
      itemStyle: { color: '#0d7377', opacity: 0.65 },
    }];
  } else if (chartType === 'bar') {
    const labels = Array.from({ length: 12 }, (_, i) => `Group_${i + 1}`);
    const vals = labels.map(() => Math.round(Math.random() * 500 + 50));
    series = [{ type: 'bar', data: vals, itemStyle: { color: '#3b6ea5' } }];
    xData.splice(0, xData.length, ...labels);
  } else if (chartType === 'histogram') {
    const bins = Array.from({ length: 20 }, (_, i) => -4 + i * 0.4);
    const counts = bins.map(() => Math.round(Math.random() * 200 + 20));
    series = [{ type: 'bar', data: counts, barWidth: '96%', itemStyle: { color: '#7c5cbf' } }];
    xData.splice(0, xData.length, ...bins.map(b => b.toFixed(1)));
  } else if (chartType === 'boxplot') {
    const groups = ['Control', 'Treated', 'Recovery'];
    const bpData = groups.map(() => {
      const d = Array.from({ length: 50 }, () => Math.random() * 10).sort((a, b) => a - b);
      const q1 = d[12], med = d[24], q3 = d[37];
      return [d[2], q1, med, q3, d[47]];
    });
    series = [{ type: 'boxplot', data: bpData, itemStyle: { color: '#c9503c', borderColor: '#a03020' } }];
    xData.splice(0, xData.length, ...groups);
  }

  const useXCategory = ['bar', 'histogram', 'boxplot'].includes(chartType);
  chart.setOption({
    backgroundColor: '#faf8f4',
    textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
    title: { text: `${yKey} vs ${xKey}`, textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 14, color: '#1c1917' }, top: 6, left: 10 },
    grid: ECHART_THEME.grid,
    toolbox: ECHART_THEME.toolbox,
    tooltip: { trigger: chartType === 'scatter' ? 'item' : 'axis' },
    xAxis: { type: useXCategory ? 'category' : 'value', data: useXCategory ? xData : undefined, name: xKey, nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
    yAxis: { type: 'value', name: yKey, nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
    series,
  });
  window.addEventListener('resize', () => chart.resize());
}
