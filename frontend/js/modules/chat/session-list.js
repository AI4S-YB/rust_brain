import { chatApi } from '../../api/chat.js';

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

export async function renderSessionListPage(container) {
  container.innerHTML = `
    <div class="module-view">
      <h2>💬 AI Copilot — Sessions</h2>
      <div class="chat-toolbar">
        <button class="btn btn-primary btn-new-chat">+ New chat</button>
      </div>
      <ul class="session-list"></ul>
    </div>`;

  container.querySelector('.btn-new-chat').addEventListener('click', async () => {
    try {
      const s = await chatApi.createSession(null);
      location.hash = `#chat/${s.id}`;
    } catch (e) {
      alert('Failed to create session: ' + e);
    }
  });

  const list = container.querySelector('.session-list');
  let idx;
  try {
    idx = await chatApi.listSessions();
  } catch (e) {
    list.innerHTML = `<li class="empty">Error loading sessions: ${escapeHtml(e)}</li>`;
    return;
  }
  if (!idx.sessions || idx.sessions.length === 0) {
    list.innerHTML = '<li class="empty">No sessions yet. Click "+ New chat" above.</li>';
    return;
  }
  idx.sessions.forEach(meta => {
    const li = document.createElement('li');
    li.className = 'session-list-row';
    li.innerHTML = `
      <span class="title">${escapeHtml(meta.title)}</span>
      <span class="count">${meta.message_count} msgs</span>
      <button class="btn-del" title="Delete">×</button>`;
    li.querySelector('.title').addEventListener('click', () => {
      location.hash = `#chat/${meta.id}`;
    });
    li.querySelector('.btn-del').addEventListener('click', async (e) => {
      e.stopPropagation();
      if (confirm('Delete this session?')) {
        try {
          await chatApi.deleteSession(meta.id);
          renderSessionListPage(container);
        } catch (err) {
          alert('Failed to delete: ' + err);
        }
      }
    });
    list.appendChild(li);
  });
}
