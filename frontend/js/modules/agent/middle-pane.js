import { agentState } from './state.js';

const COLOR_BY_BUCKET = {
  read_fs: 'gray',
  sandbox_write: 'green',
  code_run_sandbox: 'green',
  web: 'teal',
  memory_write: 'blue',
};

export function appendStreamEvent(ev) {
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  switch (ev.kind) {
    case 'text':       appendText(m, ev.delta); break;
    case 'reasoning':  appendReasoning(m, ev.delta); break;
    case 'tool_call':  appendToolCall(m, ev); break;
    case 'tool_result': appendToolResult(m, ev); break;
    case 'memory':     appendMemory(m, ev.recalled); break;
    case 'checkpoint': /* handled by right-pane */ break;
    case 'crystallize': appendCrystallize(m, ev); break;
    case 'done':       appendDone(m); break;
    case 'error':      appendError(m, ev.message); break;
  }
  m.scrollTop = m.scrollHeight;
}

function lastAssistantBlock(m) {
  const last = m.querySelector('.agent-msg-assistant:last-child .agent-text');
  if (last) return last;
  const block = document.createElement('div');
  block.className = 'agent-msg agent-msg-assistant';
  block.innerHTML = '<div class="agent-text"></div>';
  m.appendChild(block);
  return block.querySelector('.agent-text');
}
function appendText(m, delta) {
  const target = lastAssistantBlock(m);
  target.textContent += delta;
}
function appendReasoning(m, delta) {
  let r = m.querySelector('.agent-msg-assistant:last-child .agent-reasoning');
  if (!r) {
    const block = m.querySelector('.agent-msg-assistant:last-child') || lastAssistantBlock(m).parentElement;
    const div = document.createElement('details');
    div.className = 'agent-reasoning';
    div.innerHTML = '<summary>thinking…</summary><pre></pre>';
    block.appendChild(div);
    r = div;
  }
  r.querySelector('pre').textContent += delta;
}
function appendToolCall(m, ev) {
  const c = document.createElement('div');
  c.className = `agent-tool-call agent-tool-call-${ev.decision}`;
  c.dataset.callId = ev.call_id;
  c.dataset.bucket = ev.bucket;
  c.dataset.name = ev.name;
  const color = COLOR_BY_BUCKET[ev.bucket?.split(':')[0]] || 'slate';
  c.innerHTML = `
    <div class="agent-tool-head">
      <span class="agent-tool-name">${escapeHtml(ev.name)}</span>
      <span class="agent-bucket agent-bucket-${color}">${escapeHtml(ev.bucket)}</span>
      <span class="agent-decision">${ev.decision}</span>
    </div>
    <details class="agent-tool-args"><summary>args</summary><pre>${escapeHtml(JSON.stringify(ev.args, null, 2))}</pre></details>
    <div class="agent-tool-status">${ev.decision === 'allow' ? 'running…' : 'awaiting approval'}</div>`;
  m.appendChild(c);
}
function appendToolResult(m, ev) {
  const c = m.querySelector(`[data-call-id="${ev.call_id}"]`);
  if (c) {
    c.querySelector('.agent-tool-status').textContent = ev.result?.error ? `error: ${ev.result.error}` : 'done';
    const det = document.createElement('details');
    det.className = 'agent-tool-result';
    det.innerHTML = `<summary>result</summary><pre>${escapeHtml(JSON.stringify(ev.result, null, 2))}</pre>`;
    c.appendChild(det);
  }
}
function appendMemory(m, recalled) {
  agentState.recalled = recalled || [];
  if (!recalled?.length) return;
  const c = document.createElement('details');
  c.className = 'agent-memory-card';
  c.innerHTML = `<summary>Recalled ${recalled.length} memory entries</summary>
    <ul>${recalled.map(r => `<li>[${escapeHtml(r.scope)}|${escapeHtml(r.kind)}] ${escapeHtml(r.text)}</li>`).join('')}</ul>`;
  m.appendChild(c);
}
function appendCrystallize(m, ev) {
  const c = document.createElement('div');
  c.className = 'agent-crystallize';
  c.textContent = `Crystallized to ${ev.layer}/${ev.scope}: ${ev.path}`;
  m.appendChild(c);
}
function appendDone(m) {
  const c = document.createElement('div');
  c.className = 'agent-done';
  c.textContent = '— task done —';
  m.appendChild(c);
}
function appendError(m, msg) {
  const c = document.createElement('div');
  c.className = 'agent-error';
  c.textContent = `error: ${msg}`;
  m.appendChild(c);
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
