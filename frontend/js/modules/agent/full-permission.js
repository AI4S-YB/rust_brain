import { agentApi } from './api.js';
import { agentState } from './state.js';

export function renderToolbar(root) {
  root.innerHTML = `
    <div class="agent-toolbar">
      <label class="agent-fp-toggle">
        <input type="checkbox" id="agent-fp-checkbox">
        <span>Full permission</span>
      </label>
      <button id="agent-cancel-btn">Cancel run</button>
    </div>`;
  const cb = root.querySelector('#agent-fp-checkbox');
  cb.addEventListener('change', async () => {
    if (cb.checked) {
      const ok = confirm('Full permission disables every approval gate AND turns off network logging. Proceed?');
      if (!ok) { cb.checked = false; return; }
    }
    agentState.fullPermission = cb.checked;
    await agentApi.setFullPermission(agentState.projectRoot, cb.checked);
  });
  root.querySelector('#agent-cancel-btn').addEventListener('click', async () => {
    await agentApi.cancel(agentState.projectRoot);
  });
}
