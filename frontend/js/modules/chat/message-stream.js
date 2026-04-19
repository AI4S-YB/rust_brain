import { chatApi } from '../../api/chat.js';
import { createPlanCard, markStatus } from './plan-card.js';
import { createRunCard } from './run-card.js';

/**
 * Attach a chat-stream listener. Renders text deltas into an assistant bubble
 * that accretes across tokens, plus cards for tool calls and run progress.
 *
 * toolSchemasByName: optional map of toolName → JSON Schema — used by Plan
 * cards to render typed forms. Phase 1 may leave this empty; schema-form
 * falls back to a raw JSON textarea.
 */
export async function attachStream({ container, sessionId, toolSchemasByName }) {
  let currentAssistant = null;
  let rafToken = null;

  const append = (el) => {
    container.appendChild(el);
    container.scrollTop = container.scrollHeight;
  };

  const ensureAssistantBubble = () => {
    if (!currentAssistant) {
      currentAssistant = document.createElement('div');
      currentAssistant.className = 'chat-msg chat-msg-assistant';
      append(currentAssistant);
    }
    return currentAssistant;
  };

  const scheduleRender = (bubble, delta) => {
    bubble.dataset.raw = (bubble.dataset.raw || '') + delta;
    if (rafToken) return;
    rafToken = requestAnimationFrame(() => {
      bubble.textContent = bubble.dataset.raw;
      rafToken = null;
    });
  };

  return chatApi.subscribeStream(ev => {
    if (ev.session_id !== sessionId) return;
    if (ev.kind === 'Text') {
      scheduleRender(ensureAssistantBubble(), ev.delta);
    } else if (ev.kind === 'ToolCall') {
      currentAssistant = null; // assistant bubble is finished before the card
      if (ev.risk === 'read') {
        // Read-risk tools execute automatically — render a compact one-liner.
        const row = document.createElement('div');
        row.className = 'tool-auto';
        row.dataset.callId = ev.call_id;
        row.textContent = `🔧 ${ev.name}  (auto)`;
        append(row);
      } else {
        const schema = toolSchemasByName ? toolSchemasByName[ev.name] : null;
        append(createPlanCard({
          callId: ev.call_id,
          name: ev.name,
          args: ev.args,
          schema,
          risk: ev.risk,
        }));
      }
    } else if (ev.kind === 'ToolResult') {
      // If there's a card for this call_id, mark it done / show a short result.
      const card = container.querySelector(`[data-call-id="${CSS.escape(ev.call_id)}"]`);
      if (card && card.classList.contains('plan-card')) {
        markStatus(card, 'done');
      }
      if (ev.result && ev.result.run_id) {
        const rc = createRunCard({
          runId: ev.result.run_id,
          moduleId: ev.result.module_id || '',
        });
        append(rc);
      } else if (ev.result && ev.result.error) {
        const err = document.createElement('div');
        err.className = 'tool-error';
        err.textContent = `tool error: ${ev.result.error}`;
        append(err);
      } else {
        const row = document.createElement('pre');
        row.className = 'tool-result';
        const text = JSON.stringify(ev.result ?? null, null, 2);
        row.textContent = text.length > 600 ? text.slice(0, 600) + '…' : text;
        append(row);
      }
    } else if (ev.kind === 'Done') {
      currentAssistant = null;
    } else if (ev.kind === 'Error') {
      const err = document.createElement('div');
      err.className = 'chat-msg chat-msg-error';
      err.textContent = `Error: ${ev.message}`;
      append(err);
      currentAssistant = null;
    }
  });
}
