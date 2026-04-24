/* ============================================================
   RustBrain — Frontend i18n
   Tiny dictionary + t()/setLang()/applyI18n() — no deps.
   ============================================================ */
(function () {
  'use strict';

  const STORAGE_KEY = 'rustbrain.lang';
  const SUPPORTED = ['zh', 'en'];

  const DICT = window.RustBrainLocales || {};

  function resolveInitial() {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (SUPPORTED.includes(stored)) return stored;
    } catch (_) { /* localStorage disabled */ }
    const nav = (navigator.language || 'en').toLowerCase();
    return nav.startsWith('zh') ? 'zh' : 'en';
  }

  let currentLang = resolveInitial();
  document.documentElement.setAttribute('lang', currentLang === 'zh' ? 'zh-CN' : 'en');

  function lookup(dict, key) {
    const parts = key.split('.');
    let node = dict;
    for (const p of parts) {
      if (node == null || typeof node !== 'object') return undefined;
      node = node[p];
    }
    return typeof node === 'string' ? node : undefined;
  }

  function interpolate(str, vars) {
    if (!vars) return str;
    return str.replace(/\{(\w+)\}/g, (_, k) => (vars[k] != null ? String(vars[k]) : '{' + k + '}'));
  }

  function t(key, vars) {
    const s = lookup(DICT[currentLang], key)
           ?? lookup(DICT.en, key)
           ?? key;
    return interpolate(s, vars);
  }

  function applyI18n(root) {
    if (!root) return;
    root.querySelectorAll('[data-i18n]').forEach(el => {
      el.textContent = t(el.dataset.i18n);
    });
    root.querySelectorAll('[data-i18n-attr]').forEach(el => {
      el.dataset.i18nAttr.split(',').forEach(pair => {
        const [attr, key] = pair.split(':').map(s => s && s.trim());
        if (attr && key) el.setAttribute(attr, t(key));
      });
    });
  }

  function setLang(lang) {
    if (!SUPPORTED.includes(lang) || lang === currentLang) return;
    currentLang = lang;
    try { localStorage.setItem(STORAGE_KEY, lang); } catch (_) {}
    document.documentElement.setAttribute('lang', lang === 'zh' ? 'zh-CN' : 'en');
    applyI18n(document);
    window.dispatchEvent(new CustomEvent('langchange', { detail: { lang } }));
  }

  window.I18N = { t, setLang, getLang: () => currentLang, applyI18n };

  // Dev-only asymmetry check — logs any keys missing from the other locale.
  // Guarded to localhost / dev-server hosts so production consoles stay clean.
  const isDevHost = (() => {
    try {
      const h = location.hostname;
      return h === 'localhost' || h === '127.0.0.1' || h === '' || h === '[::1]';
    } catch (_) { return false; }
  })();
  if (isDevHost) {
    (function checkKeys() {
      const walk = (obj, prefix = '') => {
        const out = [];
        for (const [k, v] of Object.entries(obj)) {
          const path = prefix ? `${prefix}.${k}` : k;
          if (v && typeof v === 'object' && !Array.isArray(v)) out.push(...walk(v, path));
          else out.push(path);
        }
        return out;
      };
      const en = new Set(walk(DICT.en));
      const zh = new Set(walk(DICT.zh));
      const onlyEn = [...en].filter(k => !zh.has(k));
      const onlyZh = [...zh].filter(k => !en.has(k));
      if (onlyEn.length) console.warn('[i18n] keys only in en:', onlyEn);
      if (onlyZh.length) console.warn('[i18n] keys only in zh:', onlyZh);
    })();
  }
})();
