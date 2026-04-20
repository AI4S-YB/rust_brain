export function toggleCollapsible(trigger) {
  const c = trigger.closest('.collapsible');
  if (c) { c.classList.toggle('open'); if (window.lucide) window.lucide.createIcons(); }
}
