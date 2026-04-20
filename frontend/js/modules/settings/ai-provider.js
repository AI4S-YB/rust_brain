import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { api } from '../../core/tauri.js';

export function renderAiProviderSection() {
  const html = `
    <div class="module-panel animate-slide-up" style="animation-delay:220ms">
      <div class="panel-header"><span class="panel-title">${escapeHtml(t('settings.ai_section'))}</span></div>
      <div class="panel-body">
        <form class="settings-ai-form">
          <div class="form-row">
            <label class="form-label">${escapeHtml(t('settings.ai_provider'))}</label>
            <select name="provider_id">
              <option value="openai-compat">OpenAI-compatible</option>
            </select>
          </div>
          <div class="form-row">
            <label class="form-label">${escapeHtml(t('settings.ai_base_url'))}</label>
            <input type="url" name="base_url" value="https://api.openai.com/v1" />
          </div>
          <div class="form-row">
            <label class="form-label">${escapeHtml(t('settings.ai_model'))}</label>
            <input type="text" name="model" placeholder="gpt-4o-mini / deepseek-chat / qwen-max …" />
          </div>
          <div class="form-row">
            <label class="form-label">${escapeHtml(t('settings.ai_temperature'))}</label>
            <input type="range" name="temperature" min="0" max="2" step="0.05" value="0.2" />
            <span class="temp-readout">0.2</span>
          </div>
          <div class="form-row">
            <label class="form-label">${escapeHtml(t('settings.ai_api_key'))}</label>
            <input type="password" name="api_key" autocomplete="off" placeholder="${escapeHtml(t('settings.ai_api_key_placeholder'))}" />
          </div>
          <div class="form-actions">
            <button type="button" class="btn btn-primary ai-save">${escapeHtml(t('common.save'))}</button>
            <button type="button" class="btn ai-clear-key">${escapeHtml(t('settings.ai_clear_key'))}</button>
            <span class="ai-key-state muted"></span>
          </div>
        </form>
      </div>
    </div>`;

  async function bind(root) {
    const form = root.querySelector('.settings-ai-form');
    if (!form) return;
    const setState = (text) => {
      const el = form.querySelector('.ai-key-state');
      if (el) el.textContent = text;
    };

    try {
      const cfg = await api.invoke('ai_get_config');
      const pc = cfg && cfg.providers && cfg.providers['openai-compat'];
      if (pc) {
        if (pc.base_url) form.querySelector('[name="base_url"]').value = pc.base_url;
        if (pc.model) form.querySelector('[name="model"]').value = pc.model;
        if (pc.temperature != null) form.querySelector('[name="temperature"]').value = pc.temperature;
      }
    } catch (e) {
      console.warn('[ai_get_config] failed:', e);
    }

    const temp = form.querySelector('[name="temperature"]');
    const readout = form.querySelector('.temp-readout');
    const updateReadout = () => { if (readout) readout.textContent = temp.value; };
    if (temp && readout) {
      temp.addEventListener('input', updateReadout);
      updateReadout();
    }

    const refreshKeyState = async () => {
      try {
        const has = await api.invoke('ai_has_api_key', { providerId: 'openai-compat' });
        setState(has ? t('settings.ai_key_saved') : t('settings.ai_no_key'));
      } catch (e) {
        setState('');
      }
    };
    refreshKeyState();

    form.querySelector('.ai-save').addEventListener('click', async () => {
      const config = {
        base_url: form.querySelector('[name="base_url"]').value.trim(),
        model: form.querySelector('[name="model"]').value.trim(),
        temperature: parseFloat(form.querySelector('[name="temperature"]').value),
      };
      try {
        await api.invoke('ai_set_provider_config', {
          providerId: 'openai-compat',
          config,
        });
        const keyInput = form.querySelector('[name="api_key"]');
        const key = keyInput.value;
        if (key) {
          await api.invoke('ai_set_api_key', {
            providerId: 'openai-compat',
            key,
          });
          keyInput.value = '';
        }
        await refreshKeyState();
        alert(t('settings.ai_saved'));
      } catch (e) {
        alert(t('settings.ai_save_failed') + ': ' + e);
      }
    });

    form.querySelector('.ai-clear-key').addEventListener('click', async () => {
      try {
        await api.invoke('ai_clear_api_key', { providerId: 'openai-compat' });
        await refreshKeyState();
      } catch (e) {
        alert(t('settings.ai_clear_failed') + ': ' + e);
      }
    });
  }

  return { html, bind };
}
