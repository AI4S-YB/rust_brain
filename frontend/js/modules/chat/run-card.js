import { chatApi } from '../../api/chat.js';
import { modulesApi } from '../../api/modules.js';

const listen = (event, cb) => window.__TAURI__.event.listen(event, cb);
const TERMINAL_STATUSES = new Set(['Done', 'Failed', 'Cancelled']);

function runIdFromPayload(payload) {
  return payload?.runId || payload?.run_id || payload?.id || null;
}

function viewIdForModule(moduleId) {
  return ({
    deseq2: 'differential',
    gff_convert: 'gff-convert',
    star_index: 'star-index',
    star_align: 'star-align',
  })[moduleId] || moduleId.replace(/_/g, '-');
}

export function createRunCard({ runId, moduleId }) {
  const el = document.createElement('div');
  el.className = 'run-card';
  el.dataset.runId = runId;
  el.innerHTML = `
    <header class="run-card-header">
      <span class="run-module">${escapeHtml(moduleId || '')}</span>
      <span class="run-id">${escapeHtml(runId)}</span>
    </header>
    <div class="run-progress"><div class="run-bar"></div></div>
    <div class="run-status">starting…</div>
    <footer class="run-actions">
      <button class="btn btn-sm btn-run-cancel">Cancel run</button>
      <button class="btn btn-sm btn-run-open" hidden>View details</button>
    </footer>`;

  el.querySelector('.btn-run-cancel').addEventListener('click', () => {
    chatApi.cancelRun(runId).catch(() => {});
  });
  el.querySelector('.btn-run-open').addEventListener('click', () => {
    if (moduleId) {
      // Heuristic routing: send the user to the matching traditional view for
      // the module that produced this run. Runs whose module_id doesn't match
      // a known view fall through to a no-op click.
      location.hash = `#${viewIdForModule(moduleId)}`;
    }
  });

  // Subscribe to run-progress / run-completed for this particular run.
  // Store cleanup on the element so the caller can tear it down.
  let unProg, unDone, unFailed, pollTimer;

  const stopPolling = () => {
    if (!pollTimer) return;
    clearInterval(pollTimer);
    pollTimer = null;
  };

  const markTerminal = (status, message) => {
    stopPolling();
    el.querySelector('.btn-run-cancel').hidden = true;
    el.querySelector('.btn-run-open').hidden = !moduleId;
    if (status === 'Done') el.querySelector('.run-bar').style.width = '100%';
    el.querySelector('.run-status').textContent = message || status.toLowerCase();
    if (typeof unProg === 'function') unProg();
    if (typeof unDone === 'function') unDone();
    if (typeof unFailed === 'function') unFailed();
  };

  const pollStatus = async () => {
    try {
      const runs = await modulesApi.listRuns(moduleId || null);
      const run = (runs || []).find(r => r.id === runId);
      if (!run || !TERMINAL_STATUSES.has(run.status)) return;
      const message = run.status === 'Done'
        ? 'done'
        : `${run.status.toLowerCase()}${run.error ? `: ${run.error}` : ''}`;
      markTerminal(run.status, message);
    } catch (err) {
      console.warn(`[chat-run-card] failed to refresh ${runId}:`, err);
    }
  };

  listen('run-progress', (e) => {
    if (runIdFromPayload(e.payload) !== runId) return;
    const p = e.payload.fraction ?? 0;
    el.querySelector('.run-bar').style.width = `${Math.round(p * 100)}%`;
    el.querySelector('.run-status').textContent = e.payload.message || `${Math.round(p * 100)}%`;
  }).then(un => { unProg = un; });
  listen('run-completed', (e) => {
    if (runIdFromPayload(e.payload) !== runId) return;
    markTerminal('Done', e.payload.status || 'done');
  }).then(un => { unDone = un; });
  listen('run-failed', (e) => {
    if (runIdFromPayload(e.payload) !== runId) return;
    const err = e.payload?.error || e.payload?.message || 'failed';
    const status = String(err).toLowerCase() === 'cancelled' ? 'Cancelled' : 'Failed';
    markTerminal(status, status === 'Cancelled' ? 'cancelled' : `failed: ${err}`);
  }).then(un => { unFailed = un; });

  pollTimer = setInterval(pollStatus, 2000);
  setTimeout(pollStatus, 500);

  el._teardown = () => {
    stopPolling();
    if (typeof unProg === 'function') unProg();
    if (typeof unDone === 'function') unDone();
    if (typeof unFailed === 'function') unFailed();
  };

  return el;
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
