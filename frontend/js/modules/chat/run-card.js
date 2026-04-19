import { chatApi } from '../../api/chat.js';

const listen = (event, cb) => window.__TAURI__.event.listen(event, cb);

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
      <button class="btn btn-small btn-run-cancel">Cancel run</button>
      <button class="btn btn-small btn-run-open" hidden>View details</button>
    </footer>`;

  el.querySelector('.btn-run-cancel').addEventListener('click', () => {
    chatApi.cancelRun(runId).catch(() => {});
  });
  el.querySelector('.btn-run-open').addEventListener('click', () => {
    if (moduleId) {
      // Heuristic routing: send the user to the matching traditional view for
      // the module that produced this run. Runs whose module_id doesn't match
      // a known view fall through to a no-op click.
      const viewId = moduleId.replace(/_/g, '-');
      location.hash = `#${viewId}`;
    }
  });

  // Subscribe to run-progress / run-completed for this particular run.
  // Store cleanup on the element so the caller can tear it down.
  let unProg, unDone;
  listen('run-progress', (e) => {
    if (e.payload.runId !== runId && e.payload.run_id !== runId) return;
    const p = e.payload.fraction ?? 0;
    el.querySelector('.run-bar').style.width = `${Math.round(p * 100)}%`;
    el.querySelector('.run-status').textContent = e.payload.message || `${Math.round(p * 100)}%`;
  }).then(un => { unProg = un; });
  listen('run-completed', (e) => {
    if (e.payload.runId !== runId && e.payload.run_id !== runId) return;
    el.querySelector('.btn-run-cancel').hidden = true;
    el.querySelector('.btn-run-open').hidden = false;
    el.querySelector('.run-status').textContent = e.payload.status || 'done';
    if (typeof unProg === 'function') unProg();
    if (typeof unDone === 'function') unDone();
  }).then(un => { unDone = un; });

  el._teardown = () => {
    if (typeof unProg === 'function') unProg();
    if (typeof unDone === 'function') unDone();
  };

  return el;
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
