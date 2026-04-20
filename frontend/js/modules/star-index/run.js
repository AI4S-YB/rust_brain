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

export async function submitStarIndex(form) {
  if (isModuleBusy('star-index')) {
    cancelModuleRun('star-index');
    return;
  }
  if (!canStartModuleRun('star-index')) {
    showComputeBudgetToast('star-index');
    return;
  }
  const fd = new FormData(form);
  const extra_args = (fd.get('extra_args') || '').toString().split('\n').map(s => s.trim()).filter(Boolean);
  const params = {
    genome_fasta: fd.get('genome_fasta'),
    gtf_file:     fd.get('gtf_file'),
    threads:      parseInt(fd.get('threads'), 10) || 4,
    sjdb_overhang: parseInt(fd.get('sjdb_overhang'), 10) || 100,
    genome_sa_index_nbases: parseInt(fd.get('genome_sa_index_nbases'), 10) || 14,
    extra_args,
  };
  markModuleRunPending('star-index');
  try {
    const runId = await modulesApi.run('star_index', params);
    const started = runId ? await registerStartedRun('star-index', runId) : false;
    navigate('star-index');
    if (started) runStartedToast({ module: t(navKey('star-index')), runId });
    else if (!runId) clearModuleRunState('star-index');
  } catch (err) {
    clearModuleRunState('star-index');
    alertModal({ title: 'Error', message: 'Failed to start run: ' + err });
  }
}
