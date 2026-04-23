const api = (typeof window !== 'undefined' && window.__TAURI__?.core?.invoke)
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

/**
 * igv.js expects Reader instances with a read(chr, start, end) method returning
 * a string of bases or an array of feature objects. We route those to our Rust backend.
 */
export class TauriReferenceReader {
  constructor({ path }) { this.path = path; }
  async readSequence(chr, start, end) {
    // igv.js uses 0-based start, exclusive end. Our Rust API uses 1-based inclusive.
    const rustStart = start + 1;
    const rustEnd = end;
    const seq = await api('genome_viewer_fetch_reference_region', {
      chrom: chr, start: rustStart, end: rustEnd,
    });
    return seq;
  }
}

export class TauriFeatureReader {
  constructor({ trackId, kind }) { this.trackId = trackId; this.kind = kind; }
  async readFeatures(chr, start, end) {
    const rustStart = start + 1;
    const rustEnd = end;
    const features = await api('genome_viewer_fetch_track_features', {
      trackId: this.trackId, chrom: chr, start: rustStart, end: rustEnd,
    });
    return features.map(f => ({
      chr: f.chrom,
      start: Number(f.start) - 1, // back to 0-based for igv.js
      end: Number(f.end),
      name: f.name || '',
      strand: f.strand || '.',
      type: f.kind,
      attributes: f.attrs || {},
    }));
  }
}
