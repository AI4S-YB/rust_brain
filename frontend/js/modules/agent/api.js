const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Tauri not available'); } };
const { listen } = window.__TAURI__?.event ?? { listen: async () => () => {} };

export const agentApi = {
  startSession(projectRoot, fullPermission = false) {
    return invoke('agent_start_session', { args: { projectRoot, fullPermission } });
  },
  send(projectRoot, text)        { return invoke('agent_send', { args: { projectRoot, text } }); },
  approve(projectRoot, callId, editedArgs = null) {
    return invoke('agent_approve', { args: { projectRoot, callId, editedArgs } });
  },
  reject(projectRoot, callId, reason = null) {
    return invoke('agent_reject', { args: { projectRoot, callId, reason } });
  },
  answer(projectRoot, callId, reply) {
    return invoke('agent_answer', { args: { projectRoot, callId, reply } });
  },
  cancel(projectRoot)            { return invoke('agent_cancel', { args: { projectRoot } }); },
  setFullPermission(projectRoot, enabled) {
    return invoke('agent_set_full_permission', { args: { projectRoot, enabled } });
  },
  listArchives(projectRoot)      { return invoke('agent_list_archives', { args: { projectRoot } }); },
  loadArchive(projectRoot, archiveId) {
    return invoke('agent_load_archive', { args: { projectRoot, archiveId } });
  },
  listSkills(projectRoot)        { return invoke('agent_list_skills', { args: { projectRoot } }); },
  editMemory(path, content)      { return invoke('agent_edit_memory', { args: { path, content } }); },
};

export function onAgentStream(handler)   { return listen('agent-stream', e => handler(e.payload)); }
export function onAgentAskUser(handler)  { return listen('agent-ask-user', e => handler(e.payload)); }
