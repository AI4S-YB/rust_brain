import { modulesApi } from '../../api/modules.js';
import { navigate } from '../../core/router.js';
import { alertModal, runStartedToast } from '../../ui/modal.js';
import { t, navKey } from '../../core/i18n-helpers.js';
import {
  canStartModuleRun,
  cancelModuleRun,
  clearModuleRunState,
  isModuleBusy,
  markModuleRunPending,
  registerStartedRun,
  showComputeBudgetToast,
} from '../../core/run-controls.js';

export async function submitGffConvert(form) {
  if (isModuleBusy('gff-convert')) {
    cancelModuleRun('gff-convert');
    return;
  }
  if (!canStartModuleRun('gff-convert')) {
    showComputeBudgetToast('gff-convert');
    return;
  }
  const fd = new FormData(form);
  const extra_args = (fd.get('extra_args') || '').toString()
    .split('\n').map(s => s.trim()).filter(Boolean);
  const params = {
    input_file: fd.get('input_file'),
    target_format: fd.get('target_format'),
    extra_args,
  };
  markModuleRunPending('gff-convert');
  try {
    const runId = await modulesApi.run('gff_convert', params);
    const started = runId ? await registerStartedRun('gff-convert', runId) : false;
    navigate('gff-convert');
    if (started) runStartedToast({ module: t(navKey('gff-convert')), runId });
    else if (!runId) clearModuleRunState('gff-convert');
  } catch (err) {
    clearModuleRunState('gff-convert');
    alertModal({ title: 'Error', message: 'Failed to start run: ' + err });
  }
}
