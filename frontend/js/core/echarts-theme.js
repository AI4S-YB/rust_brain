export { ECHART_THEME } from './constants.js';

export function createChart(container) {
  return window.echarts.init(container, null, { renderer: 'canvas' });
}
