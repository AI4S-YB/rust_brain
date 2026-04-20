import { chatApi } from '../../api/chat.js';
import { renderSchemaForm } from './schema-form.js';
import { promptModal, alertModal } from '../../ui/modal.js';

export function createPlanCard({ callId, name, args, schema, risk }) {
  const el = document.createElement('div');
  el.className = `plan-card plan-card-${risk}`;
  el.dataset.callId = callId;
  el.innerHTML = `
    <header class="plan-card-header">
      <span class="plan-tool">${escapeHtml(name)}</span>
      <span class="plan-risk">${escapeHtml(risk)}</span>
    </header>
    <div class="plan-form"></div>
    <footer class="plan-actions">
      <button class="btn btn-primary btn-exec">Execute</button>
      <button class="btn btn-reject">Reject</button>
    </footer>`;

  const form = renderSchemaForm(schema || { type: 'object', properties: {} }, args);
  el.querySelector('.plan-form').appendChild(form.el);

  el.querySelector('.btn-exec').addEventListener('click', async () => {
    disableActions(el);
    try {
      await chatApi.approveTool(callId, form.getValues());
      markStatus(el, 'executing');
    } catch (e) {
      markStatus(el, 'error');
      const err = document.createElement('div');
      err.className = 'plan-card-error';
      err.textContent = 'Approve failed: ' + e;
      el.appendChild(err);
    }
  });

  el.querySelector('.btn-reject').addEventListener('click', async () => {
    const reason = await promptModal({
      title: 'Reject tool call',
      message: 'Optional reason for rejection (leave blank to skip):',
      defaultValue: '',
    });
    disableActions(el);
    try {
      await chatApi.rejectTool(callId, reason || null);
      markStatus(el, 'rejected');
    } catch (e) {
      markStatus(el, 'error');
    }
  });

  return el;
}

export function markStatus(cardEl, status) {
  cardEl.classList.add(`plan-card-${status}`);
  const footer = cardEl.querySelector('.plan-actions');
  if (footer) footer.hidden = true;
  let badge = cardEl.querySelector('.plan-status');
  if (!badge) {
    badge = document.createElement('span');
    badge.className = 'plan-status';
    cardEl.querySelector('header').appendChild(badge);
  }
  badge.textContent = status;
}

function disableActions(cardEl) {
  cardEl.querySelectorAll('.plan-actions button').forEach(b => { b.disabled = true; });
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
