function basename(path) {
  if (!path) return '';
  const parts = String(path).split(/[\\/]/);
  return parts[parts.length - 1] || String(path);
}

function dirname(path) {
  if (!path) return '';
  const normalized = String(path).replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx > 0 ? normalized.slice(0, idx) : '';
}

function normalizeModuleStatus(status) {
  switch (String(status || '').toLowerCase()) {
    case 'pass': return 'pass';
    case 'warn':
    case 'warning': return 'warn';
    case 'fail':
    case 'error': return 'fail';
    case 'not_applicable':
    case 'na':
    case 'n/a': return 'not_applicable';
    default: return 'not_applicable';
  }
}

function normalizeInputStatus(status, hasReport, error) {
  const normalized = String(status || '').toLowerCase();
  if (normalized === 'ok' || normalized === 'done' || normalized === 'pass') return 'ok';
  if (normalized === 'error' || normalized === 'failed' || normalized === 'fail') return 'error';
  if (hasReport && !error) return 'ok';
  return error ? 'error' : 'ok';
}

function deriveModuleCounts(modules) {
  const counts = { pass: 0, warn: 0, fail: 0, not_applicable: 0 };
  modules.forEach(module => {
    const status = normalizeModuleStatus(module.status);
    counts[status] = (counts[status] || 0) + 1;
  });
  return counts;
}

function deriveOverallStatus(counts) {
  if ((counts.fail || 0) > 0) return 'fail';
  if ((counts.warn || 0) > 0) return 'warn';
  return 'pass';
}

function meanQualityFromModule(module) {
  const groups = module?.data?.position_groups;
  if (!Array.isArray(groups) || groups.length === 0) return null;
  const values = groups
    .map(item => Number(item.mean))
    .filter(value => Number.isFinite(value));
  if (values.length === 0) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function normalizeFastqcReport(report, inputFile, index) {
  const modules = Array.isArray(report?.modules) ? report.modules : [];
  const moduleCounts = report?.summary?.module_counts || deriveModuleCounts(modules);
  const overallStatus = report?.summary?.overall_status || deriveOverallStatus(moduleCounts);
  const moduleMap = new Map(modules.map(module => [module.id, module]));
  const basicModule = moduleMap.get('basic_statistics');
  const basicData = basicModule?.data || {};
  const summaryBasic = report?.summary?.basic_statistics || {};
  const positionQuality = moduleMap.get('per_base_sequence_quality');

  return {
    key: inputFile || report?.input?.source_path || report?.input?.file_name || `report-${index + 1}`,
    displayName: report?.input?.file_name || basename(inputFile) || `Report ${index + 1}`,
    inputFile: inputFile || report?.input?.source_path || report?.input?.file_name || '',
    status: 'ok',
    error: null,
    schemaVersion: report?.schema_version || null,
    generatedAt: report?.generated_at || null,
    overallStatus: normalizeModuleStatus(overallStatus),
    moduleCounts,
    modules,
    moduleMap,
    basicStatistics: {
      fileName: basicData.file_name || report?.input?.file_name || basename(inputFile),
      fileType: basicData.file_type || '',
      encoding: basicData.encoding || '',
      totalSequences: summaryBasic.total_sequences ?? basicData.total_sequences ?? 0,
      filteredSequences: summaryBasic.filtered_sequences ?? basicData.filtered_sequences ?? 0,
      totalBases: summaryBasic.total_bases ?? basicData.total_bases ?? 0,
      totalBasesHuman: basicData.total_bases_human || '',
      minSequenceLength: summaryBasic.min_sequence_length ?? basicData.sequence_length?.min ?? 0,
      maxSequenceLength: summaryBasic.max_sequence_length ?? basicData.sequence_length?.max ?? 0,
      sequenceLengthDisplay: basicData.sequence_length?.display || '',
      gcPercent: summaryBasic.gc_percent ?? basicData.gc_percent ?? 0,
    },
    meanQuality: meanQualityFromModule(positionQuality),
    fastqcReport: report,
  };
}

function normalizeSchemaReports(result) {
  const summary = result?.summary || {};
  if (Array.isArray(summary.reports)) {
    return summary.reports.map((item, index) => {
      const fastqcReport = item?.fastqc_report || item?.report || item?.structured_report || null;
      if (!fastqcReport || !fastqcReport.modules) {
        return {
          key: item?.input_file || item?.file || `report-${index + 1}`,
          displayName: basename(item?.input_file || item?.file) || `Report ${index + 1}`,
          inputFile: item?.input_file || item?.file || '',
          status: normalizeInputStatus(item?.status, false, item?.error),
          error: item?.error || null,
          schemaVersion: null,
          overallStatus: 'not_applicable',
          moduleCounts: { pass: 0, warn: 0, fail: 0, not_applicable: 0 },
          modules: [],
          moduleMap: new Map(),
          basicStatistics: null,
          meanQuality: null,
          fastqcReport: null,
        };
      }

      const normalized = normalizeFastqcReport(fastqcReport, item?.input_file || item?.file, index);
      normalized.status = normalizeInputStatus(item?.status, true, item?.error);
      normalized.error = item?.error || null;
      return normalized;
    });
  }

  if (summary?.schema_version && Array.isArray(summary.modules)) {
    return [normalizeFastqcReport(summary, summary?.input?.source_path, 0)];
  }

  return [];
}

function normalizeLegacyFiles(summary) {
  const files = Array.isArray(summary?.files) ? summary.files : [];
  return files.map((item, index) => ({
    key: item?.file || `legacy-${index + 1}`,
    displayName: basename(item?.file) || `Input ${index + 1}`,
    inputFile: item?.file || '',
    status: normalizeInputStatus(item?.status, false, item?.error),
    error: item?.error || null,
  }));
}

export function normalizeQcResult(result) {
  const summary = result?.summary || {};
  const outputFiles = Array.isArray(result?.output_files) ? result.output_files : [];
  const reports = normalizeSchemaReports(result);
  const legacyFiles = normalizeLegacyFiles(summary);

  return {
    mode: reports.length > 0 ? 'schema' : 'legacy',
    log: result?.log || '',
    outputFiles,
    outputDirectory: summary?.output_directory || dirname(outputFiles[0] || ''),
    totalFiles: summary?.total_files ?? (reports.length || legacyFiles.length),
    processedOk: summary?.processed_ok ?? reports.filter(report => report.status === 'ok').length,
    files: reports.length > 0 ? reports.map(report => ({
      file: report.inputFile || report.displayName,
      status: report.status,
      error: report.error || null,
    })) : legacyFiles.map(file => ({
      file: file.inputFile || file.displayName,
      status: file.status,
      error: file.error || null,
    })),
    reports,
    legacyFiles,
  };
}
