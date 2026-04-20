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

export async function submitStarAlign(form) {
  if (isModuleBusy('star-align')) {
    cancelModuleRun('star-align');
    return;
  }
  if (!canStartModuleRun('star-align')) {
    showComputeBudgetToast('star-align');
    return;
  }
  const fd = new FormData(form);
  const splitPaths = (s) => (s || '').toString().split(/\s+/).map(x => x.trim()).filter(Boolean);
  const splitLines = (s) => (s || '').toString().split('\n').map(x => x.trim()).filter(Boolean);
  const params = {
    genome_dir:    fd.get('genome_dir'),
    reads_1:       splitPaths(fd.get('reads_1')),
    reads_2:       splitPaths(fd.get('reads_2')),
    sample_names:  splitLines(fd.get('sample_names')),
    threads:       parseInt(fd.get('threads'), 10) || 4,
    strand:        fd.get('strand') || 'unstranded',
    extra_args:    splitLines(fd.get('extra_args')),
  };
  if (params.sample_names.length === 0) delete params.sample_names;
  if (params.reads_2.length === 0) delete params.reads_2;
  markModuleRunPending('star-align');
  try {
    const runId = await modulesApi.run('star_align', params);
    const started = runId ? await registerStartedRun('star-align', runId) : false;
    navigate('star-align');
    if (started) runStartedToast({ module: t(navKey('star-align')), runId });
    else if (!runId) clearModuleRunState('star-align');
  } catch (err) {
    clearModuleRunState('star-align');
    alertModal({ title: 'Error', message: 'Failed to start run: ' + err });
  }
}
