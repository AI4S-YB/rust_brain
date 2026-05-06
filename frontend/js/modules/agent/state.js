// Per-session in-memory state. Only ONE active session shown at a time;
// multi-session UX is out of scope for v0.3.
export const agentState = {
  projectRoot: null,
  sessionId: null,
  messages: [],     // {role, content, tool_calls?, ...} normalized for render
  recalled: [],     // RecallCandidate[]
  todo: [],         // TodoEntry[]
  pendingAsks: {},  // call_id -> prompt
  archives: [],     // ArchiveListEntry[]
  skills: { global: [], project: [] },
  fullPermission: false,
};
