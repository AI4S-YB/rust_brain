import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';

export function renderGffConvertResult(result, runId) {
  const s = result.summary || {};
  const out = (result.output_files && result.output_files[0]) || s.output || '';
  const fmt = String(s.target_format || '').toUpperCase();
  return `
    <div class="run-result-card">
      <h3>${t('gff_convert.converted_heading', { format: escapeHtml(fmt) })}</h3>
      <dl class="result-kv">
        <dt>${t('gff_convert.kv_input')}</dt><dd class="path" title="${escapeHtml(s.input || '')}">${escapeHtml(s.input || '')}</dd>
        <dt>${t('gff_convert.kv_output')}</dt><dd class="path" title="${escapeHtml(out)}">${escapeHtml(out)}</dd>
        <dt>${t('gff_convert.kv_input_size')}</dt><dd>${s.input_bytes ?? '?'} ${t('gff_convert.kv_bytes_suffix')}</dd>
        <dt>${t('gff_convert.kv_output_size')}</dt><dd>${s.output_bytes ?? '?'} ${t('gff_convert.kv_bytes_suffix')}</dd>
        <dt>${t('gff_convert.kv_elapsed')}</dt><dd>${s.elapsed_ms ?? '?'} ${t('gff_convert.kv_ms_suffix')}</dd>
      </dl>
      <button type="button" data-gff-use-in-star="${escapeHtml(out)}">${t('gff_convert.use_in_star')}</button>
    </div>
  `;
}
