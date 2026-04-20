import { ECHART_THEME, createChart } from '../../core/echarts-theme.js';

function initChart(container) {
  if (!container || !window.echarts) return null;
  const current = window.echarts.getInstanceByDom(container);
  if (current) current.dispose();
  return createChart(container);
}

function applyChart(container, option) {
  const chart = initChart(container);
  if (!chart) return;
  chart.setOption(option);
}

export function renderPerBaseQualityChart(container, module) {
  const points = module?.data?.position_groups;
  if (!container || !Array.isArray(points) || points.length === 0) return;

  applyChart(container, {
    ...ECHART_THEME,
    title: { text: 'Per Base Sequence Quality', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: points.map(point => point.label),
      name: 'Position (bp)',
    },
    yAxis: {
      type: 'value',
      name: 'Phred Score',
      min: 0,
      max: 42,
    },
    series: [
      {
        name: '90th Percentile',
        type: 'line',
        data: points.map(point => point.percentile_10),
        lineStyle: { width: 0 },
        symbol: 'none',
        stack: 'quality-band',
        areaStyle: { opacity: 0 },
      },
      {
        name: '10th-90th Range',
        type: 'line',
        data: points.map(point => point.percentile_90 - point.percentile_10),
        lineStyle: { width: 0 },
        symbol: 'none',
        stack: 'quality-band',
        areaStyle: { color: 'rgba(13, 115, 119, 0.10)' },
      },
      {
        name: 'Mean',
        type: 'line',
        data: points.map(point => point.mean),
        lineStyle: { color: '#0d7377', width: 2.5 },
        symbol: 'none',
        smooth: true,
      },
      {
        name: 'Median',
        type: 'line',
        data: points.map(point => point.median),
        lineStyle: { color: '#c9503c', width: 1.5, type: 'dashed' },
        symbol: 'none',
        smooth: true,
      },
    ],
    legend: { data: ['Mean', 'Median'], top: 10, right: 10 },
    grid: ECHART_THEME.grid,
  });
}

export function renderPerSequenceGcChart(container, module) {
  const points = module?.data?.distribution;
  if (!container || !Array.isArray(points) || points.length === 0) return;

  applyChart(container, {
    ...ECHART_THEME,
    title: { text: 'Per Sequence GC Content', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: points.map(point => point.gc_percent),
      name: 'GC Content (%)',
    },
    yAxis: {
      type: 'value',
      name: 'Count',
    },
    series: [
      {
        name: 'Observed',
        type: 'line',
        data: points.map(point => point.observed_count),
        symbol: 'none',
        lineStyle: { color: '#3b6ea5', width: 2.2 },
        smooth: true,
      },
      {
        name: 'Theoretical',
        type: 'line',
        data: points.map(point => point.theoretical_count),
        symbol: 'none',
        lineStyle: { color: '#b8860b', width: 1.8, type: 'dashed' },
        smooth: true,
      },
    ],
    legend: { data: ['Observed', 'Theoretical'], top: 10, right: 10 },
    grid: ECHART_THEME.grid,
  });
}

export function renderSequenceLengthChart(container, module) {
  const points = module?.data?.distribution;
  if (!container || !Array.isArray(points) || points.length === 0) return;

  applyChart(container, {
    ...ECHART_THEME,
    title: { text: 'Sequence Length Distribution', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: points.map(point => point.label),
      name: 'Length (bp)',
    },
    yAxis: {
      type: 'value',
      name: 'Count',
    },
    series: [
      {
        name: 'Count',
        type: 'bar',
        data: points.map(point => point.count),
        itemStyle: { color: 'rgba(59, 110, 165, 0.72)' },
        barWidth: '72%',
      },
    ],
    grid: ECHART_THEME.grid,
  });
}

export function renderDuplicationChart(container, module) {
  const points = module?.data?.distribution;
  if (!container || !Array.isArray(points) || points.length === 0) return;

  applyChart(container, {
    ...ECHART_THEME,
    title: { text: 'Sequence Duplication Levels', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: points.map(point => point.label),
      name: 'Duplication Level',
    },
    yAxis: {
      type: 'value',
      name: '% of Total',
      min: 0,
      max: 100,
    },
    series: [
      {
        name: 'Percentage',
        type: 'bar',
        data: points.map(point => point.percentage_of_total),
        itemStyle: { color: 'rgba(201, 80, 60, 0.72)' },
        barWidth: '72%',
      },
    ],
    grid: ECHART_THEME.grid,
  });
}
