import { escapeHtml } from './escape.js';
import { t } from '../core/i18n-helpers.js';

export function promptModal({ title, message, defaultValue = '', placeholder = '', okLabel, cancelLabel } = {}) {
  return new Promise((resolve) => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    const titleId = 'modal-title-' + Math.random().toString(36).slice(2, 8);
    backdrop.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="${titleId}">
        ${title ? `<h3 class="modal-title" id="${titleId}">${escapeHtml(title)}</h3>` : ''}
        ${message ? `<p class="modal-message">${escapeHtml(message)}</p>` : ''}
        <input type="text" class="modal-input" value="${escapeHtml(defaultValue)}" placeholder="${escapeHtml(placeholder)}" />
        <div class="modal-actions">
          <button type="button" class="btn btn-secondary modal-btn-cancel">${escapeHtml(cancelLabel || t('common.cancel'))}</button>
          <button type="button" class="btn btn-primary modal-btn-ok">${escapeHtml(okLabel || t('common.ok'))}</button>
        </div>
      </div>
    `;
    document.body.appendChild(backdrop);

    const input = backdrop.querySelector('.modal-input');
    const okBtn = backdrop.querySelector('.modal-btn-ok');
    const cancelBtn = backdrop.querySelector('.modal-btn-cancel');
    const prevFocus = document.activeElement;

    const finish = (value) => {
      document.removeEventListener('keydown', onKey, true);
      backdrop.remove();
      if (prevFocus && typeof prevFocus.focus === 'function') prevFocus.focus();
      resolve(value);
    };
    const onKey = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); finish(null); }
      else if (e.key === 'Enter') { e.preventDefault(); finish(input.value); }
    };

    okBtn.addEventListener('click', () => finish(input.value));
    cancelBtn.addEventListener('click', () => finish(null));
    backdrop.addEventListener('click', (e) => { if (e.target === backdrop) finish(null); });
    document.addEventListener('keydown', onKey, true);

    requestAnimationFrame(() => { input.focus(); input.select(); });
  });
}
