// Thin wrapper over Tauri invoke/listen for chat_* / ai_* commands and the
// two chat events. Keeps view code decoupled from window.__TAURI__.

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const listen = (event, cb) => window.__TAURI__.event.listen(event, cb);

export const chatApi = {
  listSessions:     (scope = 'project')      => invoke('chat_list_sessions', { scope }),
  createSession:    (title, scope = 'project') => invoke('chat_create_session', { title, scope }),
  getSession:       (sessionId, scope = 'project') => invoke('chat_get_session', { sessionId, scope }),
  deleteSession:    (sessionId, scope = 'project') => invoke('chat_delete_session', { sessionId, scope }),
  renameSession:    (sessionId, title, scope = 'project') => invoke('chat_rename_session', { sessionId, title, scope }),
  sendMessage:      (sessionId, text, scope = 'project') => invoke('chat_send_message', { sessionId, text, scope }),
  approveTool:      (callId, editedArgs)     => invoke('chat_approve_tool', { callId, editedArgs }),
  rejectTool:       (callId, reason)         => invoke('chat_reject_tool', { callId, reason }),
  cancelTurn:       (sessionId, scope = 'project') => invoke('chat_cancel_turn', { sessionId, scope }),
  cancelRun:        (runId)                  => invoke('chat_cancel_run', { runId }),
  subscribeStream:  (cb)                     => listen('chat-stream', (e) => cb(e.payload)),
  subscribeUpdated: (cb)                     => listen('chat-session-updated', (e) => cb(e.payload)),
};

export const aiApi = {
  getConfig:          ()                       => invoke('ai_get_config'),
  setProviderConfig:  (providerId, config)     => invoke('ai_set_provider_config', { providerId, config }),
  setDefaultProvider: (providerId)             => invoke('ai_set_default_provider', { providerId }),
  setApiKey:          (providerId, key)        => invoke('ai_set_api_key', { providerId, key }),
  clearApiKey:        (providerId)             => invoke('ai_clear_api_key', { providerId }),
  hasApiKey:          (providerId)             => invoke('ai_has_api_key', { providerId }),
  backendInfo:        ()                       => invoke('ai_backend_info'),
};
