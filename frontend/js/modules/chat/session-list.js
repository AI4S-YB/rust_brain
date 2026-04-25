import { chatApi } from '../../api/chat.js';
import { alertModal, confirmModal } from '../../ui/modal.js';
import { state } from '../../core/state.js';
import { projectNew, projectOpen } from '../dashboard/project.js';

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function normalizeScope(scope) {
  return scope === 'direct' ? 'direct' : 'project';
}

export async function renderSessionListPage(container, options = {}) {
  const scope = normalizeScope(options.scope || (state.projectOpen ? 'project' : 'direct'));
  const directActive = scope === 'direct';
  container.innerHTML = `
    <div class="module-view">
      <h2>AI Copilot</h2>
      <div class="chat-mode-tabs">
        <button class="chat-mode-tab ${!directActive ? 'active' : ''}" data-chat-mode="project">Project AI</button>
        <button class="chat-mode-tab ${directActive ? 'active' : ''}" data-chat-mode="direct">Direct AI</button>
      </div>
      <p class="chat-mode-note">
        ${directActive
          ? 'Direct AI works without a project. It can answer general questions but cannot inspect files or run analyses.'
          : 'Project AI uses the currently open project and can inspect project state or propose analysis runs.'}
      </p>
      <div class="chat-toolbar">
        <button class="btn btn-primary btn-new-chat">+ New chat</button>
        ${!directActive ? `
          <button class="btn btn-secondary btn-open-project">${state.projectOpen ? 'Switch project' : 'Open project'}</button>
          <button class="btn btn-secondary btn-new-project">New project</button>
        ` : ''}
      </div>
      <ul class="session-list"></ul>
    </div>`;

  container.querySelectorAll('[data-chat-mode]').forEach(btn => {
    btn.addEventListener('click', () => {
      const next = btn.dataset.chatMode;
      location.hash = `#chat/${next}`;
    });
  });

  container.querySelector('.btn-open-project')?.addEventListener('click', async () => {
    const info = await projectOpen();
    if (info) location.hash = '#chat/project';
  });
  container.querySelector('.btn-new-project')?.addEventListener('click', async () => {
    const info = await projectNew();
    if (info) location.hash = '#chat/project';
  });

  container.querySelector('.btn-new-chat').addEventListener('click', async () => {
    if (!directActive && !state.projectOpen) {
      alertModal({ title: 'Project required', message: 'Open or create a project before starting a Project AI chat.' });
      return;
    }
    try {
      const s = await chatApi.createSession(null, scope);
      location.hash = `#chat/${scope}/${s.id}`;
    } catch (e) {
      alertModal({ title: 'Error', message: 'Failed to create session: ' + e });
    }
  });

  const list = container.querySelector('.session-list');
  if (!directActive && !state.projectOpen) {
    list.innerHTML = '<li class="empty">No project is open. Open or create a project to use Project AI, or switch to Direct AI.</li>';
    return;
  }
  let idx;
  try {
    idx = await chatApi.listSessions(scope);
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
      location.hash = `#chat/${scope}/${meta.id}`;
    });
    li.querySelector('.btn-del').addEventListener('click', async (e) => {
      e.stopPropagation();
      const ok = await confirmModal({ title: 'Delete session?', message: 'This will remove the session permanently.' });
      if (ok) {
        try {
          await chatApi.deleteSession(meta.id, scope);
          renderSessionListPage(container, { scope });
        } catch (err) {
          alertModal({ title: 'Error', message: 'Failed to delete: ' + err });
        }
      }
    });
    list.appendChild(li);
  });
}
