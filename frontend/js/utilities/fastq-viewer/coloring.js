const BASE_COLOR = { A: '#2d8659', C: '#3b6ea5', G: '#b8860b', T: '#c9503c', N: '#a8a29e' };

export function colorSeq(seq) {
  let out = '';
  for (const ch of seq) {
    const color = BASE_COLOR[ch.toUpperCase()] || '#57534e';
    out += `<span style="color:${color}">${ch}</span>`;
  }
  return out;
}

// Phred 33-encoded ASCII → Q score → HSL red→green
export function colorQual(qual) {
  let out = '';
  for (const ch of qual) {
    const q = Math.max(0, Math.min(40, ch.charCodeAt(0) - 33));
    // 0 → red (0deg), 40 → green (120deg)
    const hue = Math.round((q / 40) * 120);
    out += `<span style="background:hsl(${hue},60%,85%);padding:0 1px">${escape(ch)}</span>`;
  }
  return out;
}

function escape(s) {
  return s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
