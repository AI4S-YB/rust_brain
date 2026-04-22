const STORAGE_KEY = 'rustbrain.font-size';
const SIZES = { small: 12, medium: 14, large: 16, xlarge: 18 };
const DEFAULT = 'medium';

export function getFontSize() {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v && SIZES[v]) return v;
  } catch (_) {}
  return DEFAULT;
}

export function applyFontSize(size) {
  const key = SIZES[size] ? size : getFontSize();
  document.documentElement.style.fontSize = `${SIZES[key]}px`;
}

export function setFontSize(size) {
  if (!SIZES[size] || size === getFontSize()) return;
  try { localStorage.setItem(STORAGE_KEY, size); } catch (_) {}
  applyFontSize(size);
  window.dispatchEvent(new CustomEvent('fontsizechange', { detail: { size } }));
}
