import { chatApi } from '../../api/chat.js';

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function bubbleForRole(role) {
  const el = document.createElement('div');
  el.className = `chat-msg chat-msg-${role}`;
  return el;
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
    if (m.role === 'tool') return; // hide raw tool results until T33
    const el = bubbleForRole(m.role);
    el.textContent = m.content || '';
    messagesEl.appendChild(el);
  });
  messagesEl.scrollTop = messagesEl.scrollHeight;

  // Naive stream listener — T33 replaces this with the real dispatcher.
  let currentAssistant = null;
  const unlisten = await chatApi.subscribeStream(ev => {
    if (ev.session_id !== sessionId) return;
    if (ev.kind === 'Text') {
      if (!currentAssistant) {
        currentAssistant = bubbleForRole('assistant');
        messagesEl.appendChild(currentAssistant);
      }
      currentAssistant.textContent = (currentAssistant.textContent || '') + ev.delta;
      messagesEl.scrollTop = messagesEl.scrollHeight;
    } else if (ev.kind === 'Done') {
      currentAssistant = null;
    } else if (ev.kind === 'Error') {
      const err = document.createElement('div');
      err.className = 'chat-msg chat-msg-error';
      err.textContent = `Error: ${ev.message}`;
      messagesEl.appendChild(err);
    }
  });

  // Clean up the listener when navigating away.
  const hashHandler = () => {
    if (!location.hash.startsWith(`#chat/${sessionId}`)) {
      if (typeof unlisten === 'function') unlisten();
      window.removeEventListener('hashchange', hashHandler);
    }
  };
  window.addEventListener('hashchange', hashHandler);

  container.querySelector('.btn-send').addEventListener('click', async () => {
    const ta = container.querySelector('.chat-input');
    const text = ta.value.trim();
    if (!text) return;
    const userEl = bubbleForRole('user');
    userEl.textContent = text;
    messagesEl.appendChild(userEl);
    ta.value = '';
    messagesEl.scrollTop = messagesEl.scrollHeight;
    try {
      await chatApi.sendMessage(sessionId, text);
    } catch (e) {
      const err = bubbleForRole('error');
      err.textContent = 'Send failed: ' + e;
      messagesEl.appendChild(err);
    }
  });

  container.querySelector('.btn-stop').addEventListener('click', () => {
    chatApi.cancelTurn(sessionId).catch(() => {});
  });
}
