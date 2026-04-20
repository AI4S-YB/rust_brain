import { state } from '../../core/state.js';
import { modulesApi } from '../../api/modules.js';
import { navigate } from '../../core/router.js';
import { alertModal, runStartedToast } from '../../ui/modal.js';
import { t, navKey } from '../../core/i18n-helpers.js';

export async function submitStarAlign(form) {
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
  try {
    const runId = await modulesApi.run('star_align', params);
    state.runIdToModule[runId] = 'star-align';
    state.currentRunId = runId;
    navigate('star-align');
    runStartedToast({ module: t(navKey('star-align')), runId });
  } catch (err) {
    alertModal({ title: 'Error', message: 'Failed to start run: ' + err });
  }
}
