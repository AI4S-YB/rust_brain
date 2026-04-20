export const t = (k, v) => (window.I18N ? window.I18N.t(k, v) : k);
export const navKey = (id) => 'nav.' + String(id).replace(/-/g, '_');
export const getLang = () => (window.I18N ? window.I18N.getLang() : 'en');
export const setLang = (lang) => { if (window.I18N) window.I18N.setLang(lang); };
export const applyI18n = (root) => { if (window.I18N) window.I18N.applyI18n(root); };
