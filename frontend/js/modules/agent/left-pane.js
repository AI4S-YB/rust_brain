import { agentState } from './state.js';
import { agentApi } from './api.js';

export async function renderLeftPane(root) {
  if (!agentState.projectRoot) {
    root.innerHTML = '<p>Open a project to see archives.</p>';
    return;
  }
  agentState.archives = await agentApi.listArchives(agentState.projectRoot);
  root.innerHTML = `
    <div class="agent-left-header">
      <h3>Research history</h3>
      <button id="agent-new-research">New research</button>
    </div>
    <ul class="agent-archive-list">
      ${agentState.archives.map(a => `
        <li class="agent-archive-item" data-id="${a.id}">
          <div class="agent-archive-summary">${escapeHtml(a.summary)}</div>
          <div class="agent-archive-meta">
            <span class="agent-archive-outcome agent-archive-outcome-${a.outcome}">${a.outcome}</span>
            <span class="agent-archive-time">${a.started_at.slice(0,16).replace('T',' ')}</span>
          </div>
        </li>`).join('')}
    </ul>`;
  // v0.3 limitation: this does not finalize the in-flight session on the backend.
  // If a session is already alive for the project, agent_start_session will reject.
  // The user must cancel from the toolbar (T14) first to truly start fresh.
  root.querySelector('#agent-new-research').addEventListener('click', async () => {
    agentState.sessionId = null;
    const r = await agentApi.startSession(agentState.projectRoot);
    agentState.sessionId = r.session_id;
    document.getElementById('agent-msgs').innerHTML = '';
    agentState.messages = [];
  });
}

function escapeHtml(s) {
  return (s||'').replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
