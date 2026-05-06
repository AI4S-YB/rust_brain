import { agentApi } from './api.js';
import { agentState } from './state.js';

export function attachApprovalHandlers(root) {
  root.addEventListener('click', async e => {
    const approveBtn = e.target.closest('[data-approve]');
    if (approveBtn) {
      const card = approveBtn.closest('.agent-tool-call');
      await agentApi.approve(agentState.projectRoot, card.dataset.callId);
      card.querySelector('.agent-tool-status').textContent = 'approved — running…';
      return;
    }
    const rejectBtn = e.target.closest('[data-reject]');
    if (rejectBtn) {
      const card = rejectBtn.closest('.agent-tool-call');
      const reason = prompt('Reason (optional)?') || null;
      await agentApi.reject(agentState.projectRoot, card.dataset.callId, reason);
      card.querySelector('.agent-tool-status').textContent = 'rejected';
      return;
    }
    const askReply = e.target.closest('.agent-ask-user [data-call-id]');
    if (askReply) {
      const callId = askReply.dataset.callId;
      const reply = prompt(`Agent asks: ${agentState.pendingAsks[callId] || ''}`) || '';
      await agentApi.answer(agentState.projectRoot, callId, reply);
      delete agentState.pendingAsks[callId];
      askReply.closest('.agent-ask-user').textContent = `(replied) ${reply}`;
    }
  });
}
