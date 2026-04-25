import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { api } from '../../core/tauri.js';
import { alertModal } from '../../ui/modal.js';

export function renderAiProviderSection() {
  const html = `
    <div class="module-panel animate-slide-up" style="animation-delay:220ms">
      <div class="panel-header">
        <span class="panel-title">${escapeHtml(t('settings.ai_section'))}</span>
        <span class="ai-key-state" data-state="unknown"></span>
      </div>
      <div class="panel-body">
        <p class="settings-intro">${escapeHtml(t('settings.ai_section_desc'))}</p>
        <form class="settings-ai-form">
          <div class="form-group">
            <label class="form-label">${escapeHtml(t('settings.ai_provider'))}</label>
            <select class="form-select" name="provider_id">
              <option value="openai-compat">OpenAI-compatible</option>
            </select>
            <span class="form-hint">${escapeHtml(t('settings.ai_provider_hint'))}</span>
          </div>

          <div class="form-group">
            <label class="form-label">${escapeHtml(t('settings.ai_base_url'))}</label>
            <input type="url" class="form-input" name="base_url" value="https://api.openai.com/v1" />
            <span class="form-hint">${escapeHtml(t('settings.ai_base_url_hint'))}</span>
          </div>

          <div class="form-group">
            <label class="form-label">${escapeHtml(t('settings.ai_model'))}</label>
            <input type="text" class="form-input" name="model" placeholder="gpt-4o-mini / deepseek-v4-pro / qwen-max …" />
            <span class="form-hint">${escapeHtml(t('settings.ai_model_hint'))}</span>
          </div>

          <div class="form-group">
            <label class="form-label">
              ${escapeHtml(t('settings.ai_temperature'))}
              <span class="temp-readout">0.2</span>
            </label>
            <input type="range" class="form-range" name="temperature" min="0" max="2" step="0.05" value="0.2" />
            <span class="form-hint">${escapeHtml(t('settings.ai_temperature_hint'))}</span>
          </div>

          <div class="form-group">
            <label class="form-label">
              <input type="checkbox" name="thinking_enabled" />
              ${escapeHtml(t('settings.ai_thinking'))}
            </label>
            <select class="form-select" name="reasoning_effort">
              <option value="high">high</option>
              <option value="max">max</option>
            </select>
            <span class="form-hint">${escapeHtml(t('settings.ai_thinking_hint'))}</span>
          </div>

          <div class="form-group">
            <label class="form-label">${escapeHtml(t('settings.ai_api_key'))}</label>
            <input type="password" class="form-input" name="api_key" autocomplete="off" placeholder="${escapeHtml(t('settings.ai_api_key_placeholder'))}" />
            <span class="form-hint">${escapeHtml(t('settings.ai_api_key_hint'))}</span>
          </div>

          <div class="form-actions ai-form-actions">
            <button type="button" class="btn btn-primary ai-save">${escapeHtml(t('common.save'))}</button>
            <button type="button" class="btn btn-secondary ai-test">${escapeHtml(t('settings.ai_test'))}</button>
            <button type="button" class="btn btn-ghost ai-clear-key">${escapeHtml(t('settings.ai_clear_key'))}</button>
          </div>
        </form>
      </div>
    </div>`;

  async function bind(root) {
    const form = root.querySelector('.settings-ai-form');
    if (!form) return;
    const badge = root.querySelector('.ai-key-state');
    const setState = (text, kind) => {
      if (!badge) return;
      badge.textContent = text;
      badge.dataset.state = kind || 'unknown';
    };

    try {
      const cfg = await api.invoke('ai_get_config');
      const pc = cfg && cfg.providers && cfg.providers['openai-compat'];
      if (pc) {
        if (pc.base_url) form.querySelector('[name="base_url"]').value = pc.base_url;
        if (pc.model) form.querySelector('[name="model"]').value = pc.model;
        if (pc.temperature != null) form.querySelector('[name="temperature"]').value = pc.temperature;
        if (pc.thinking_enabled != null) {
          form.querySelector('[name="thinking_enabled"]').checked = !!pc.thinking_enabled;
        } else if ((pc.base_url || '').includes('api.deepseek.com') && (pc.model || '').startsWith('deepseek')) {
          form.querySelector('[name="thinking_enabled"]').checked = true;
        }
        if (pc.reasoning_effort) form.querySelector('[name="reasoning_effort"]').value = pc.reasoning_effort;
      }
    } catch (e) {
      console.warn('[ai_get_config] failed:', e);
    }

    const baseUrlInput = form.querySelector('[name="base_url"]');
    const modelInput = form.querySelector('[name="model"]');
    const thinkingInput = form.querySelector('[name="thinking_enabled"]');
    const maybeEnableThinkingForDeepSeek = () => {
      const base = (baseUrlInput.value || '').trim();
      const model = (modelInput.value || '').trim();
      if (base.includes('api.deepseek.com') && model.startsWith('deepseek')) {
        thinkingInput.checked = true;
      }
    };
    baseUrlInput.addEventListener('change', maybeEnableThinkingForDeepSeek);
    modelInput.addEventListener('change', maybeEnableThinkingForDeepSeek);

    const temp = form.querySelector('[name="temperature"]');
    const readout = form.querySelector('.temp-readout');
    const updateReadout = () => { if (readout) readout.textContent = Number(temp.value).toFixed(2); };
    if (temp && readout) {
      temp.addEventListener('input', updateReadout);
      updateReadout();
    }

    const refreshKeyState = async () => {
      try {
        const has = await api.invoke('ai_has_api_key', { providerId: 'openai-compat' });
        setState(has ? t('settings.ai_key_saved') : t('settings.ai_no_key'), has ? 'saved' : 'missing');
      } catch (e) {
        setState('', 'unknown');
      }
    };
    refreshKeyState();

    form.querySelector('.ai-save').addEventListener('click', async () => {
      const config = {
        base_url: form.querySelector('[name="base_url"]').value.trim(),
        model: form.querySelector('[name="model"]').value.trim(),
        temperature: parseFloat(form.querySelector('[name="temperature"]').value),
        thinking_enabled: form.querySelector('[name="thinking_enabled"]').checked,
        reasoning_effort: form.querySelector('[name="reasoning_effort"]').value,
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
        alertModal({ message: t('settings.ai_saved') });
      } catch (e) {
        alertModal({ message: t('settings.ai_save_failed') + ': ' + e });
      }
    });

    form.querySelector('.ai-test').addEventListener('click', async (e) => {
      const btn = e.currentTarget;
      const originalLabel = btn.textContent;
      btn.disabled = true;
      btn.textContent = t('settings.ai_testing');
      try {
        const apiKey = form.querySelector('[name="api_key"]').value;
        const reply = await api.invoke('ai_test_connection', {
          providerId: 'openai-compat',
          baseUrl: form.querySelector('[name="base_url"]').value.trim(),
          model: form.querySelector('[name="model"]').value.trim(),
          temperature: parseFloat(form.querySelector('[name="temperature"]').value),
          thinkingEnabled: form.querySelector('[name="thinking_enabled"]').checked,
          reasoningEffort: form.querySelector('[name="reasoning_effort"]').value,
          apiKey: apiKey || null,
        });
        alertModal({ message: t('settings.ai_test_ok').replace('{reply}', reply) });
      } catch (err) {
        alertModal({ message: t('settings.ai_test_failed') + ': ' + err });
      } finally {
        btn.disabled = false;
        btn.textContent = originalLabel;
      }
    });

    form.querySelector('.ai-clear-key').addEventListener('click', async () => {
      try {
        await api.invoke('ai_clear_api_key', { providerId: 'openai-compat' });
        await refreshKeyState();
      } catch (e) {
        alertModal({ message: t('settings.ai_clear_failed') + ': ' + e });
      }
    });
  }

  return { html, bind };
}
