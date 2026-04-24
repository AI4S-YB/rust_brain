import { escapeHtml } from './escape.js';
import { t } from '../core/i18n-helpers.js';

export function projectNewModal() {
  return new Promise((resolve) => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    const titleId = 'pn-title-' + Math.random().toString(36).slice(2, 8);
    backdrop.innerHTML = `
      <div class="modal" role="dialog" aria-labelledby="${titleId}" aria-modal="true">
        <h3 id="${titleId}" class="modal-title">${escapeHtml(t('project.new'))}</h3>
        <label class="modal-label">
          <span>${escapeHtml(t('project.prompt_name'))}</span>
          <input type="text" class="modal-input" autofocus />
        </label>
        <fieldset class="modal-view-group">
          <legend>${escapeHtml(t('project.default_view_legend'))}</legend>
          <label>
            <input type="radio" name="default_view" value="ai" />
            <strong>${escapeHtml(t('project.default_view_ai'))}</strong>
            <small class="muted"> — ${escapeHtml(t('project.default_view_ai_hint'))}</small>
          </label>
          <label>
            <input type="radio" name="default_view" value="manual" checked />
            <strong>${escapeHtml(t('project.default_view_manual'))}</strong>
            <small class="muted"> — ${escapeHtml(t('project.default_view_manual_hint'))}</small>
          </label>
          <p class="modal-hint">${escapeHtml(t('project.default_view_note'))}</p>
        </fieldset>
        <div class="modal-actions">
          <button class="btn btn-primary modal-ok">${escapeHtml(t('common.ok'))}</button>
          <button class="btn modal-cancel">${escapeHtml(t('common.cancel'))}</button>
        </div>
      </div>`;
    document.body.appendChild(backdrop);

    const input = backdrop.querySelector('.modal-input');
    const ok = backdrop.querySelector('.modal-ok');
    const cancel = backdrop.querySelector('.modal-cancel');
    let settled = false;

    const close = (result) => {
      if (settled) return;
      settled = true;
      backdrop.remove();
      resolve(result);
    };
    ok.addEventListener('click', () => {
      const name = input.value.trim();
      if (!name) { input.focus(); return; }
      const sel = backdrop.querySelector('input[name="default_view"]:checked');
      close({ name, default_view: sel ? sel.value : 'manual' });
    });
    cancel.addEventListener('click', () => close(null));
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') ok.click();
      if (e.key === 'Escape') close(null);
    });
    input.focus();
  });
}
