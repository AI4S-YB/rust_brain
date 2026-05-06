import { agentState } from './state.js';

export function renderRightPane(root) {
  root.innerHTML = `
    <section class="agent-right-section" id="agent-todo-section">
      <h4>Working checkpoint</h4>
      <ul class="agent-todo-list" id="agent-todo-list"><li class="empty">no todo yet</li></ul>
    </section>
    <section class="agent-right-section" id="agent-sandbox-section">
      <h4>Sandbox</h4>
      <pre class="agent-sandbox-tree" id="agent-sandbox-tree">(empty)</pre>
    </section>
    <section class="agent-right-section" id="agent-net-section">
      <h4>Network log</h4>
      <pre class="agent-net-log" id="agent-net-log">(disabled)</pre>
    </section>`;
}

export function updateCheckpointTodo(todo) {
  agentState.todo = todo;
  const ul = document.getElementById('agent-todo-list');
  if (!ul) return;
  if (!todo?.length) {
    ul.innerHTML = '<li class="empty">no todo yet</li>';
    return;
  }
  ul.innerHTML = todo.map(t =>
    `<li class="${t.done ? 'done' : 'open'}">${t.done ? '✔' : '○'} ${escapeHtml(t.text)}</li>`
  ).join('');
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
