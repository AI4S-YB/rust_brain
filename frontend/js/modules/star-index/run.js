import { state } from '../../core/state.js';
import { modulesApi } from '../../api/modules.js';
import { navigate } from '../../core/router.js';

export async function submitStarIndex(form) {
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
  try {
    const runId = await modulesApi.run('star_index', params);
    state.runIdToModule[runId] = 'star_index';
    navigate('star-index');
  } catch (err) {
    alert('Failed to start run: ' + err);
  }
}
