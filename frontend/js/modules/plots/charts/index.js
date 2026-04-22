// Chart registry — order here determines the order of type buttons in the UI.
// To add a new chart type, create charts/{id}.js matching the per-chart contract
// in ./common.js, then append its namespace import to CHART_REGISTRY below.

import * as volcano     from './volcano.js';
import * as ma          from './ma.js';
import * as heatmap     from './heatmap.js';
import * as correlation from './correlation.js';
import * as box         from './box.js';
import * as gbox        from './gbox.js';
import * as violin      from './violin.js';
import * as gviolin     from './gviolin.js';
import * as ridge       from './ridge.js';
import * as strip       from './strip.js';
import * as scatter     from './scatter.js';
import * as bubble      from './bubble.js';
import * as bar         from './bar.js';
import * as lollipop    from './lollipop.js';

export const CHART_REGISTRY = [
  volcano, ma,
  heatmap, correlation,
  box, gbox,
  violin, gviolin,
  ridge, strip,
  scatter, bubble,
  bar, lollipop,
];

export function findChart(id) {
  return CHART_REGISTRY.find(c => c.meta.id === id);
}
