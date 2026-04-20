import { state } from '../../core/state.js';
import { modulesApi } from '../../api/modules.js';
import { navigate } from '../../core/router.js';
import { alertModal } from '../../ui/modal.js';

export async function submitGffConvert(form) {
  const fd = new FormData(form);
  const extra_args = (fd.get('extra_args') || '').toString()
    .split('\n').map(s => s.trim()).filter(Boolean);
  const params = {
    input_file: fd.get('input_file'),
    target_format: fd.get('target_format'),
    extra_args,
  };
  try {
    const runId = await modulesApi.run('gff_convert', params);
    state.runIdToModule[runId] = 'gff-convert';
    navigate('gff-convert');
  } catch (err) {
    alertModal({ title: 'Error', message: 'Failed to start run: ' + err });
  }
}
