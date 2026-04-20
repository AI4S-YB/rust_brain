import { api } from './tauri.js';
import { t } from './i18n-helpers.js';
import { navigate } from './router.js';
import { appendRunLog } from '../ui/log-panel.js';

export function installRuntimeListeners() {
  api.listen('run-progress', event => {
    const st = document.getElementById('statusText');
    const log = document.querySelector('.log-output');
    if (st) st.textContent = event.payload?.message || (t('status.running_prefix') + '…');
    if (log) log.innerHTML += `\n<span class="log-info">[INFO]</span> ${event.payload?.message || ''}`;
  });

  api.listen('run-completed', event => {
    const st = document.getElementById('statusText');
    const js = document.getElementById('jobStatus');
    if (st) st.textContent = t('status.ready');
    if (js) js.textContent = t('status.no_jobs');
    if (event.payload?.module) navigate(event.payload.module);
  });

  api.listen('run-failed', event => {
    const st = document.getElementById('statusText');
    if (st) st.textContent = `${t('status.error_prefix')}: ${event.payload?.message || t('status.run_failed')}`;
    console.error('[run-failed]', event.payload);
  });

  if (window.__TAURI__?.event) {
    window.__TAURI__.event.listen('run-log', (e) => {
      const { runId, line, stream } = e.payload || {};
      if (runId) appendRunLog(runId, line, stream);
    });
  }
}
