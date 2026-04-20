import { state } from '../core/state.js';
import { LOG_BUFFER_MAX } from '../core/constants.js';
import { t } from '../core/i18n-helpers.js';
import { escapeHtml } from './escape.js';

export function appendRunLog(runId, line, stream) {
  const panelKey = state.runIdToModule[runId] || runId;
  const buf = (state.logsByRun[panelKey] = state.logsByRun[panelKey] || []);
  buf.push({ line, stream });
  while (buf.length > LOG_BUFFER_MAX) buf.shift();
  const panel = document.querySelector(`[data-log-panel="${panelKey}"] pre`);
  if (panel) {
    const prefix = stream === 'stderr' ? '' : '[out] ';
    panel.textContent += prefix + line + '\n';
    if (!panel.dataset.userScrolled) {
      panel.scrollTop = panel.scrollHeight;
    }
  }
}

export function renderLogPanel(panelKey) {
  const existing = state.logsByRun[panelKey] || [];
  const text = existing.map(e => (e.stream === 'stderr' ? '' : '[out] ') + e.line).join('\n');
  return `<details class="log-panel" data-log-panel="${panelKey}">
    <summary>${t('common.log_panel')}</summary>
    <pre>${escapeHtml(text)}</pre>
  </details>`;
}

export function installLogScrollWatch() {
  document.addEventListener('scroll', (e) => {
    const pre = e.target;
    if (pre.tagName !== 'PRE' || !pre.closest('[data-log-panel]')) return;
    const nearBottom = pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20;
    if (nearBottom) delete pre.dataset.userScrolled;
    else pre.dataset.userScrolled = '1';
  }, true);
}
