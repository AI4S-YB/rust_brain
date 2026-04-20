import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { normalizeQcResult } from './normalize.js';
import {
  renderDuplicationChart,
  renderPerBaseQualityChart,
  renderPerSequenceGcChart,
  renderSequenceLengthChart,
} from './charts.js';

function sanitizeIdPart(value) {
  return String(value || 'current').replace(/[^a-zA-Z0-9_-]+/g, '-');
}

function formatCount(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '-';
  return Math.round(value).toLocaleString();
}

function formatNumber(value, digits = 1) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '-';
  return value.toFixed(digits);
}

function formatPValue(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '-';
  if (value === 0) return '0';
  if (value < 0.001) return value.toExponential(2);
  return value.toFixed(4);
}

function renderInputStatusBadge(status) {
  const normalized = String(status || '').toLowerCase();
  if (normalized === 'ok') {
    return `<span class="badge badge-green">${t('qc.status_ok')}</span>`;
  }
  if (normalized === 'error') {
    return `<span class="badge badge-coral">${t('qc.status_error')}</span>`;
  }
  return `<span class="badge badge-muted">${escapeHtml(status || '-')}</span>`;
}

function renderModuleStatusBadge(status) {
  switch (String(status || '').toLowerCase()) {
    case 'pass':
      return `<span class="badge badge-green">${t('qc.status_pass')}</span>`;
    case 'warn':
      return `<span class="badge badge-gold">${t('qc.status_warn')}</span>`;
    case 'fail':
      return `<span class="badge badge-coral">${t('qc.status_fail')}</span>`;
    default:
      return `<span class="badge badge-muted">${t('qc.status_not_applicable')}</span>`;
  }
}

function renderRunMetrics(vm) {
  return `
    <div class="results-summary">
      <div class="result-metric">
        <div class="result-metric-value">${vm.totalFiles}</div>
        <div class="result-metric-label">${t('qc.metric_total_files')}</div>
      </div>
      <div class="result-metric">
        <div class="result-metric-value">${vm.processedOk}</div>
        <div class="result-metric-label">${t('qc.metric_processed_ok')}</div>
      </div>
      <div class="result-metric">
        <div class="result-metric-value">${vm.outputFiles.length}</div>
        <div class="result-metric-label">${t('qc.metric_output_files')}</div>
      </div>
    </div>
  `;
}

function renderFilesStatusTable(files) {
  const rows = files.length
    ? files.map(item => `
      <tr>
        <td class="path" title="${escapeHtml(item.file || '')}">${escapeHtml(item.file || '')}</td>
        <td>${renderInputStatusBadge(item.status)}</td>
        <td>${item.error ? escapeHtml(item.error) : '<span class="text-muted">-</span>'}</td>
      </tr>
    `).join('')
    : `<tr><td colspan="3"><em>${t('status.no_runs')}</em></td></tr>`;

  return `
    <table class="data-table">
      <thead>
        <tr>
          <th>${t('qc.col_file')}</th>
          <th>${t('qc.col_status')}</th>
          <th>${t('qc.col_error')}</th>
        </tr>
      </thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}

function renderOutputFiles(outputFiles) {
  if (!outputFiles.length) {
    return `<p><em>${t('qc.output_files_empty')}</em></p>`;
  }
  return `<ul class="run-result-list">
    ${outputFiles.map(path => `<li class="path" title="${escapeHtml(path)}">${escapeHtml(path)}</li>`).join('')}
  </ul>`;
}

function renderLegacyResult(vm) {
  const outputDirectory = vm.outputDirectory || '-';
  return `
    <div class="run-result-card">
      ${renderRunMetrics(vm)}
      <dl class="result-kv">
        <dt>${t('qc.output_directory_label')}</dt>
        <dd class="path" title="${escapeHtml(outputDirectory)}">${escapeHtml(outputDirectory)}</dd>
      </dl>

      <h3>${t('qc.files_status_heading')}</h3>
      ${renderFilesStatusTable(vm.files)}

      <h3>${t('qc.output_files_label')}</h3>
      ${renderOutputFiles(vm.outputFiles)}

      <h3>${t('qc.tab_log')}</h3>
      <div class="log-output">${escapeHtml(vm.log)}</div>
    </div>
  `;
}

function metricCard(value, label) {
  return `
    <div class="result-metric">
      <div class="result-metric-value">${value}</div>
      <div class="result-metric-label">${label}</div>
    </div>
  `;
}

function renderBasicStats(report) {
  const stats = report?.basicStatistics;
  if (!stats) {
    return `<p><em>${t('qc.structured_report_missing')}</em></p>`;
  }

  return `
    <dl class="result-kv">
      <dt>${t('qc.col_file')}</dt>
      <dd class="path" title="${escapeHtml(stats.fileName || '')}">${escapeHtml(stats.fileName || '-')}</dd>
      <dt>${t('qc.basic_stat_file_type')}</dt>
      <dd>${escapeHtml(stats.fileType || '-')}</dd>
      <dt>${t('qc.basic_stat_encoding')}</dt>
      <dd>${escapeHtml(stats.encoding || '-')}</dd>
      <dt>${t('qc.metric_total_reads')}</dt>
      <dd>${formatCount(stats.totalSequences)}</dd>
      <dt>${t('qc.basic_stat_filtered_sequences')}</dt>
      <dd>${formatCount(stats.filteredSequences)}</dd>
      <dt>${t('qc.basic_stat_total_bases')}</dt>
      <dd>${stats.totalBasesHuman ? escapeHtml(stats.totalBasesHuman) : formatCount(stats.totalBases)}</dd>
      <dt>${t('qc.metric_read_length')}</dt>
      <dd>${escapeHtml(stats.sequenceLengthDisplay || '-')}</dd>
      <dt>${t('qc.metric_gc_content')}</dt>
      <dd>${formatNumber(stats.gcPercent, 1)}%</dd>
    </dl>
  `;
}

function renderModuleTable(report) {
  const modules = Array.isArray(report?.modules) ? report.modules : [];
  const rows = modules.length
    ? modules.map(module => `
      <tr>
        <td>${escapeHtml(module.name || module.id || '-')}</td>
        <td>${renderModuleStatusBadge(module.status)}</td>
      </tr>
    `).join('')
    : `<tr><td colspan="2"><em>${t('qc.structured_report_missing')}</em></td></tr>`;

  return `
    <table class="data-table">
      <thead>
        <tr>
          <th>${t('qc.col_module')}</th>
          <th>${t('qc.col_status')}</th>
        </tr>
      </thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}

function renderOverrepresentedTable(report) {
  const module = report?.moduleMap?.get('overrepresented_sequences');
  const rows = module?.data?.records || [];
  if (!rows.length) {
    return `<p><em>${t('status.no_runs')}</em></p>`;
  }
  return `
    <table class="data-table">
      <thead>
        <tr>
          <th>${t('qc.col_sequence')}</th>
          <th>${t('qc.col_count')}</th>
          <th>${t('qc.col_percentage')}</th>
          <th>${t('qc.col_possible_source')}</th>
        </tr>
      </thead>
      <tbody>
        ${rows.map(row => `
          <tr>
            <td class="path" title="${escapeHtml(row.sequence)}">${escapeHtml(row.sequence)}</td>
            <td>${formatCount(row.count)}</td>
            <td>${formatNumber(row.percentage, 2)}%</td>
            <td>${escapeHtml(row.possible_source || '-')}</td>
          </tr>
        `).join('')}
      </tbody>
    </table>
  `;
}

function renderAdapterTable(report) {
  const module = report?.moduleMap?.get('adapter_content');
  const data = module?.data;
  if (!data) {
    return `<p><em>${t('qc.structured_report_missing')}</em></p>`;
  }
  if (data.read_length_too_short) {
    return `<p><em>${t('qc.read_length_too_short')}</em></p>`;
  }
  const rows = Array.isArray(data.series) ? data.series : [];
  if (!rows.length) {
    return `<p><em>${t('status.no_runs')}</em></p>`;
  }

  return `
    <table class="data-table">
      <thead>
        <tr>
          <th>${t('qc.col_adapter')}</th>
          <th>${t('qc.col_sequence')}</th>
          <th>${t('qc.col_max_content')}</th>
        </tr>
      </thead>
      <tbody>
        ${rows.map(row => {
          const maxValue = Math.max(...(row.values || [0]));
          return `
            <tr>
              <td>${escapeHtml(row.adapter_name || '-')}</td>
              <td class="path" title="${escapeHtml(row.adapter_sequence || '')}">${escapeHtml(row.adapter_sequence || '-')}</td>
              <td>${formatNumber(maxValue, 2)}%</td>
            </tr>
          `;
        }).join('')}
      </tbody>
    </table>
  `;
}

function renderKmerTable(report) {
  const module = report?.moduleMap?.get('kmer_content');
  const rows = module?.data?.records || [];
  if (!rows.length) {
    return `<p><em>${t('status.no_runs')}</em></p>`;
  }

  return `
    <table class="data-table">
      <thead>
        <tr>
          <th>${t('qc.col_sequence')}</th>
          <th>${t('qc.col_count')}</th>
          <th>${t('qc.col_p_value')}</th>
          <th>${t('qc.col_obs_exp_max')}</th>
          <th>${t('qc.col_position')}</th>
        </tr>
      </thead>
      <tbody>
        ${rows.map(row => `
          <tr>
            <td>${escapeHtml(row.sequence || '-')}</td>
            <td>${formatCount(row.estimated_count)}</td>
            <td>${formatPValue(row.p_value)}</td>
            <td>${formatNumber(row.obs_exp_max, 2)}</td>
            <td>${escapeHtml(row.max_obs_exp_position || '-')}</td>
          </tr>
        `).join('')}
      </tbody>
    </table>
  `;
}

function renderSelectedReport(rootId, vm, report, activeTab) {
  const outputDirectory = vm.outputDirectory || '-';
  const gcMetric = report?.basicStatistics
    ? `${formatNumber(report.basicStatistics.gcPercent, 1)}%`
    : '-';
  const metrics = report?.fastqcReport ? `
    <div class="results-summary">
      ${metricCard(renderModuleStatusBadge(report.overallStatus), t('qc.metric_overall'))}
      ${metricCard(formatCount(report.basicStatistics?.totalSequences), t('qc.metric_total_reads'))}
      ${metricCard(report.meanQuality == null ? '-' : formatNumber(report.meanQuality, 1), t('qc.metric_mean_quality'))}
      ${metricCard(gcMetric, t('qc.metric_gc_content'))}
    </div>
  ` : `<p><em>${report?.error ? escapeHtml(report.error) : t('qc.structured_report_missing')}</em></p>`;

  const tabId = name => `${rootId}-${name}`;
  const isActive = name => activeTab === name ? ' active' : '';

  return `
    <div class="qc-result-toolbar">
      <div class="form-group">
        <label class="form-label" for="${tabId('select')}">${t('qc.report_file_label')}</label>
        <select class="form-select" id="${tabId('select')}">
          ${vm.reports.map(item => `<option value="${escapeHtml(item.key)}"${item.key === report.key ? ' selected' : ''}>${escapeHtml(item.displayName)}</option>`).join('')}
        </select>
      </div>
      <div class="qc-selected-file">
        <div class="result-metric-label">${t('qc.selected_report_label')}</div>
        <div class="qc-selected-file-value path" title="${escapeHtml(report.inputFile || report.displayName)}">${escapeHtml(report.inputFile || report.displayName)}</div>
      </div>
    </div>

    ${renderRunMetrics(vm)}

    <div class="tabs">
      <div class="tab${isActive('quality')}" data-tab="${tabId('quality')}" data-qc-tab="quality">${t('qc.tab_quality')}</div>
      <div class="tab${isActive('summary')}" data-tab="${tabId('summary')}" data-qc-tab="summary">${t('qc.tab_summary')}</div>
      <div class="tab${isActive('modules')}" data-tab="${tabId('modules')}" data-qc-tab="modules">${t('qc.tab_modules')}</div>
      <div class="tab${isActive('artifacts')}" data-tab="${tabId('artifacts')}" data-qc-tab="artifacts">${t('qc.tab_artifacts')}</div>
      <div class="tab${isActive('log')}" data-tab="${tabId('log')}" data-qc-tab="log">${t('qc.tab_log')}</div>
    </div>

    <div class="tab-content${isActive('quality')}" data-tab="${tabId('quality')}">
      ${report.fastqcReport ? `
        <div class="qc-chart-grid">
          <div class="qc-chart-card">
            <h3>${t('qc.chart_per_base_quality')}</h3>
            <div class="chart-container" id="${tabId('chart-quality')}" style="height: 300px;"></div>
          </div>
          <div class="qc-chart-card">
            <h3>${t('qc.chart_gc_distribution')}</h3>
            <div class="chart-container" id="${tabId('chart-gc')}" style="height: 300px;"></div>
          </div>
          <div class="qc-chart-card">
            <h3>${t('qc.chart_length_distribution')}</h3>
            <div class="chart-container" id="${tabId('chart-length')}" style="height: 300px;"></div>
          </div>
          <div class="qc-chart-card">
            <h3>${t('qc.chart_duplication')}</h3>
            <div class="chart-container" id="${tabId('chart-duplication')}" style="height: 300px;"></div>
          </div>
        </div>
      ` : `<p><em>${report.error ? escapeHtml(report.error) : t('qc.structured_report_missing')}</em></p>`}
    </div>

    <div class="tab-content${isActive('summary')}" data-tab="${tabId('summary')}">
      ${metrics}
      <h3>${t('qc.basic_stats_heading')}</h3>
      ${renderBasicStats(report)}
      <h3>${t('qc.files_status_heading')}</h3>
      ${renderFilesStatusTable(vm.files)}
    </div>

    <div class="tab-content${isActive('modules')}" data-tab="${tabId('modules')}">
      <h3>${t('qc.module_status_heading')}</h3>
      ${renderModuleTable(report)}
      <h3>${t('qc.overrepresented_heading')}</h3>
      ${renderOverrepresentedTable(report)}
      <h3>${t('qc.adapter_content_heading')}</h3>
      ${renderAdapterTable(report)}
      <h3>${t('qc.kmer_content_heading')}</h3>
      ${renderKmerTable(report)}
    </div>

    <div class="tab-content${isActive('artifacts')}" data-tab="${tabId('artifacts')}">
      <dl class="result-kv">
        <dt>${t('qc.output_directory_label')}</dt>
        <dd class="path" title="${escapeHtml(outputDirectory)}">${escapeHtml(outputDirectory)}</dd>
      </dl>
      <h3>${t('qc.output_files_label')}</h3>
      ${renderOutputFiles(vm.outputFiles)}
    </div>

    <div class="tab-content${isActive('log')}" data-tab="${tabId('log')}">
      <div class="log-output">${escapeHtml(vm.log)}</div>
    </div>
  `;
}

function mountSchemaResult(rootId, vm, selectedKey, activeTab = 'quality') {
  const root = document.getElementById(rootId);
  if (!root) return;

  const defaultReport = vm.reports.find(item => item.fastqcReport) || vm.reports[0];
  const report = vm.reports.find(item => item.key === selectedKey) || defaultReport;
  if (!report) {
    root.innerHTML = renderLegacyResult(vm);
    return;
  }

  root.setAttribute('data-tab-scope', 'qc-result');
  root.innerHTML = renderSelectedReport(rootId, vm, report, activeTab);

  const select = document.getElementById(`${rootId}-select`);
  if (select) {
    select.addEventListener('change', () => {
      const nextActiveTab = root.querySelector('.tab.active')?.dataset.qcTab || 'quality';
      mountSchemaResult(rootId, vm, select.value, nextActiveTab);
    });
  }

  if (!report.fastqcReport) return;

  renderPerBaseQualityChart(
    document.getElementById(`${rootId}-chart-quality`),
    report.moduleMap.get('per_base_sequence_quality')
  );
  renderPerSequenceGcChart(
    document.getElementById(`${rootId}-chart-gc`),
    report.moduleMap.get('per_sequence_gc_content')
  );
  renderSequenceLengthChart(
    document.getElementById(`${rootId}-chart-length`),
    report.moduleMap.get('sequence_length_distribution')
  );
  renderDuplicationChart(
    document.getElementById(`${rootId}-chart-duplication`),
    report.moduleMap.get('sequence_duplication_levels')
  );
}

export function renderQcResult(result, runId) {
  const vm = normalizeQcResult(result);
  if (vm.mode === 'legacy' || vm.reports.length === 0) {
    return renderLegacyResult(vm);
  }

  const rootId = `qc-result-${sanitizeIdPart(runId)}`;
  const initialReport = vm.reports.find(item => item.fastqcReport) || vm.reports[0];
  setTimeout(() => mountSchemaResult(rootId, vm, initialReport?.key, 'quality'), 0);
  return `<div id="${rootId}" class="run-result-card qc-result-card"></div>`;
}
