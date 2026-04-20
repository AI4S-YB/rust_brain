export const api = {
  invoke(command, args) { return window.__TAURI__.core.invoke(command, args); },
  listen(event, callback) { return window.__TAURI__.event.listen(event, callback); },
};
