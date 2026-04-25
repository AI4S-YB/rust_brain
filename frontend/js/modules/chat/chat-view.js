import { chatApi } from '../../api/chat.js';
import { attachStream } from './message-stream.js';

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function bubbleForRole(role) {
  const el = document.createElement('div');
  el.className = `chat-msg chat-msg-${role}`;
  return el;
}

function reasoningBlock(text) {
  const details = document.createElement('details');
  details.className = 'chat-reasoning';
  const summary = document.createElement('summary');
  summary.textContent = 'Thinking';
  const body = document.createElement('pre');
  body.textContent = text || '';
  details.append(summary, body);
  return details;
}

export async function renderChatView(container, sessionId) {
  let session;
  try {
    session = await chatApi.getSession(sessionId);
  } catch (e) {
    container.innerHTML = `<div class="module-view"><p>Could not load session: ${escapeHtml(e)}</p></div>`;
    return;
  }
  if (!session) {
    container.innerHTML = `<div class="module-view"><p>Session not found.</p><p><a href="#chat">Back to sessions</a></p></div>`;
    return;
  }

  container.innerHTML = `
    <div class="module-view chat-shell">
      <header class="chat-header">
        <a class="chat-back" href="#chat">←</a>
        <h2 class="chat-title">${escapeHtml(session.title)}</h2>
        <span class="chat-provider">${escapeHtml(session.provider_snapshot?.model ?? '')}</span>
      </header>
      <div class="chat-messages"></div>
      <footer class="chat-input-bar">
        <textarea class="chat-input" placeholder="Describe what you want to analyse..." rows="2"></textarea>
        <button class="btn btn-primary btn-send">Send</button>
        <button class="btn btn-stop" hidden>Stop</button>
      </footer>
    </div>`;

  const messagesEl = container.querySelector('.chat-messages');
  // Render existing messages.
  (session.messages || []).forEach(m => {
    if (m.role === 'tool') return;
    if (m.reasoning_content) {
      messagesEl.appendChild(reasoningBlock(m.reasoning_content));
    }
    const el = bubbleForRole(m.role);
    el.textContent = m.content || '';
    messagesEl.appendChild(el);
  });
  messagesEl.scrollTop = messagesEl.scrollHeight;

  const sendBtn = container.querySelector('.btn-send');
  const stopBtn = container.querySelector('.btn-stop');
  const setSending = (sending) => {
    sendBtn.disabled = sending;
    stopBtn.hidden = !sending;
  };

  // Attach the full streaming dispatcher.
  const unlisten = await attachStream({
    container: messagesEl,
    sessionId,
    // toolSchemasByName — left empty for Phase 1; Plan Card falls back to raw-JSON form.
    toolSchemasByName: {},
    onDone: () => setSending(false),
    onError: () => setSending(false),
  });

  // Clean up the listener when navigating away.
  const hashHandler = () => {
    if (!location.hash.startsWith(`#chat/${sessionId}`)) {
      if (typeof unlisten === 'function') unlisten();
      window.removeEventListener('hashchange', hashHandler);
    }
  };
  window.addEventListener('hashchange', hashHandler);

  sendBtn.addEventListener('click', async () => {
    const ta = container.querySelector('.chat-input');
    const text = ta.value.trim();
    if (!text) return;
    setSending(true);
    const userEl = bubbleForRole('user');
    userEl.textContent = text;
    messagesEl.appendChild(userEl);
    ta.value = '';
    messagesEl.scrollTop = messagesEl.scrollHeight;
    try {
      await chatApi.sendMessage(sessionId, text);
    } catch (e) {
      setSending(false);
      const err = bubbleForRole('error');
      err.textContent = 'Send failed: ' + e;
      messagesEl.appendChild(err);
    }
  });

  stopBtn.addEventListener('click', () => {
    chatApi.cancelTurn(sessionId).catch(() => {});
  });
}
