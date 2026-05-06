import { agentState } from './state.js';
import { agentApi, onAgentStream, onAgentAskUser } from './api.js';
import { state as appState } from '../../core/state.js';

export async function renderAgentView(content) {
  content.innerHTML = `
    <div class="agent-shell">
      <aside class="agent-left" id="agent-left">left pane (archives) — wired in Task 9</aside>
      <section class="agent-mid" id="agent-mid">
        <div class="agent-msgs" id="agent-msgs"></div>
        <form class="agent-input" id="agent-input-form">
          <textarea id="agent-input" placeholder="Ask the agent..." rows="3"></textarea>
          <button type="submit">Send</button>
        </form>
      </section>
      <aside class="agent-right" id="agent-right">right pane — wired in Task 11</aside>
    </div>`;
  const projectRoot = appState.project?.root_dir;
  if (!projectRoot) {
    content.querySelector('#agent-msgs').innerHTML = '<p>Open a project first.</p>';
    return;
  }
  agentState.projectRoot = projectRoot;
  if (!agentState.sessionId) {
    const r = await agentApi.startSession(projectRoot);
    agentState.sessionId = r.session_id;
  }
  if (!window.__agentListening) {
    window.__agentListening = true;
    onAgentStream(ev => handleStream(ev));
    onAgentAskUser(req => handleAskUser(req));
  }
  document.getElementById('agent-input-form').addEventListener('submit', async e => {
    e.preventDefault();
    const ta = document.getElementById('agent-input');
    const text = ta.value.trim();
    if (!text) return;
    ta.value = '';
    pushMessage({ role: 'user', content: text });
    await agentApi.send(agentState.projectRoot, text);
  });
}

function handleStream(ev) {
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  const line = document.createElement('div');
  line.className = 'agent-event-debug';
  line.textContent = JSON.stringify(ev);
  m.appendChild(line);
  m.scrollTop = m.scrollHeight;
}

function handleAskUser(req) {
  agentState.pendingAsks[req.call_id] = req.prompt;
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  const line = document.createElement('div');
  line.className = 'agent-ask-user';
  line.innerHTML = `<strong>Agent asks:</strong> ${escapeHtml(req.prompt)} <button data-call-id="${req.call_id}">Reply</button>`;
  m.appendChild(line);
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}

function pushMessage(m) {
  agentState.messages.push(m);
  const target = document.getElementById('agent-msgs');
  if (!target) return;
  const div = document.createElement('div');
  div.className = `agent-msg agent-msg-${m.role}`;
  div.textContent = `[${m.role}] ${m.content}`;
  target.appendChild(div);
  target.scrollTop = target.scrollHeight;
}
