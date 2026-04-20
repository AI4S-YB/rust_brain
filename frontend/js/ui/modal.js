import { escapeHtml } from './escape.js';
import { t } from '../core/i18n-helpers.js';

let toastContainer;

function getToastContainer() {
  if (toastContainer && document.body.contains(toastContainer)) return toastContainer;
  toastContainer = document.createElement('div');
  toastContainer.className = 'toast-stack';
  toastContainer.setAttribute('aria-live', 'polite');
  toastContainer.setAttribute('aria-atomic', 'false');
  document.body.appendChild(toastContainer);
  return toastContainer;
}

function dismissToast(toast) {
  if (!toast || toast.dataset.closing === 'true') return;
  toast.dataset.closing = 'true';
  toast.classList.add('is-closing');
  window.setTimeout(() => toast.remove(), 220);
}

function openModal({ title, message, input, buttons }) {
  return new Promise((resolve) => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop';
    const titleId = 'modal-title-' + Math.random().toString(36).slice(2, 8);

    const inputHtml = input
      ? `<input type="text" class="modal-input" value="${escapeHtml(input.defaultValue ?? '')}" placeholder="${escapeHtml(input.placeholder ?? '')}" />`
      : '';

    const buttonsHtml = buttons.map((b, i) => {
      const cls = b.variant === 'primary' ? 'btn btn-primary' : 'btn btn-secondary';
      return `<button type="button" class="${cls}" data-modal-btn="${i}">${escapeHtml(b.label)}</button>`;
    }).join('');

    backdrop.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="${titleId}">
        ${title ? `<h3 class="modal-title" id="${titleId}">${escapeHtml(title)}</h3>` : ''}
        ${message ? `<p class="modal-message">${escapeHtml(message)}</p>` : ''}
        ${inputHtml}
        <div class="modal-actions">${buttonsHtml}</div>
      </div>
    `;
    document.body.appendChild(backdrop);

    const inputEl = backdrop.querySelector('.modal-input');
    const prevFocus = document.activeElement;

    const finish = (result) => {
      document.removeEventListener('keydown', onKey, true);
      backdrop.remove();
      if (prevFocus && typeof prevFocus.focus === 'function') prevFocus.focus();
      resolve(result);
    };

    const onKey = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        const cancelBtn = buttons.find(b => b.role === 'cancel');
        if (cancelBtn) finish(cancelBtn.value);
      } else if (e.key === 'Enter' && inputEl && document.activeElement === inputEl) {
        e.preventDefault();
        const okBtn = buttons.find(b => b.role === 'ok');
        if (okBtn) finish(typeof okBtn.value === 'function' ? okBtn.value(inputEl.value) : okBtn.value);
      }
    };

    backdrop.querySelectorAll('[data-modal-btn]').forEach((btn, i) => {
      btn.addEventListener('click', () => {
        const cfg = buttons[i];
        finish(typeof cfg.value === 'function' ? cfg.value(inputEl ? inputEl.value : undefined) : cfg.value);
      });
    });
    backdrop.addEventListener('click', (e) => {
      if (e.target === backdrop) {
        const cancelBtn = buttons.find(b => b.role === 'cancel');
        if (cancelBtn) finish(cancelBtn.value);
      }
    });
    document.addEventListener('keydown', onKey, true);

    if (inputEl) {
      requestAnimationFrame(() => { inputEl.focus(); inputEl.select(); });
    } else {
      const primary = backdrop.querySelector('.btn-primary');
      if (primary) requestAnimationFrame(() => primary.focus());
    }
  });
}

export function promptModal({ title, message, defaultValue = '', placeholder = '', okLabel, cancelLabel } = {}) {
  return openModal({
    title,
    message,
    input: { defaultValue, placeholder },
    buttons: [
      { label: cancelLabel || t('common.cancel'), role: 'cancel', value: null },
      { label: okLabel || t('common.ok'), variant: 'primary', role: 'ok', value: (inputValue) => inputValue },
    ],
  });
}

export function alertModal({ title, message, okLabel } = {}) {
  return openModal({
    title,
    message,
    buttons: [
      { label: okLabel || t('common.ok'), variant: 'primary', role: 'ok', value: undefined },
    ],
  });
}

export function showToast({ title, message, duration = 3200 } = {}) {
  const container = getToastContainer();
  const toast = document.createElement('section');
  toast.className = 'toast';
  toast.setAttribute('role', 'status');
  toast.innerHTML = `
    <div class="toast-body">
      ${title ? `<div class="toast-title">${escapeHtml(title)}</div>` : ''}
      ${message ? `<div class="toast-message">${escapeHtml(message)}</div>` : ''}
    </div>
    <button type="button" class="toast-close" aria-label="${escapeHtml(t('common.close'))}">
      ×
    </button>
  `;
  container.appendChild(toast);

  const closeBtn = toast.querySelector('.toast-close');
  const timeoutId = window.setTimeout(() => dismissToast(toast), duration);
  closeBtn?.addEventListener('click', () => {
    window.clearTimeout(timeoutId);
    dismissToast(toast);
  });

  return toast;
}

export function runStartedToast({ module, runId } = {}) {
  return showToast({
    title: t('status.run_started_title'),
    message: t('status.run_started_message', {
      module: module || t('common.module_not_found'),
      runId: runId || '-',
    }),
  });
}

export function confirmModal({ title, message, okLabel, cancelLabel } = {}) {
  return openModal({
    title,
    message,
    buttons: [
      { label: cancelLabel || t('common.cancel'), role: 'cancel', value: false },
      { label: okLabel || t('common.ok'), variant: 'primary', role: 'ok', value: true },
    ],
  });
}
