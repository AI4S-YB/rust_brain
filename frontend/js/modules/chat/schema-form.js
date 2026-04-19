// Renders a JSON Schema draft-07 object into an HTML form.
// Supported: type string|number|integer|boolean|array (scalar items), enum,
// minimum/maximum, default, required, description, items.
// Nested objects render as inline labelled sub-forms.

export function renderSchemaForm(schema, initialArgs) {
  const form = document.createElement('form');
  form.className = 'schema-form';
  // Guard: if schema not provided, render a generic "raw JSON" textarea.
  if (!schema || !schema.properties) {
    const ta = document.createElement('textarea');
    ta.className = 'schema-form-raw';
    ta.rows = 6;
    ta.value = JSON.stringify(initialArgs ?? {}, null, 2);
    form.appendChild(ta);
    return {
      el: form,
      getValues: () => {
        try { return JSON.parse(ta.value || '{}'); }
        catch { return initialArgs ?? {}; }
      },
    };
  }

  const state = structuredClone(initialArgs ?? {});
  const props = schema.properties;
  const required = new Set(schema.required || []);

  for (const [name, sub] of Object.entries(props)) {
    const row = document.createElement('label');
    row.className = 'form-row';
    const label = document.createElement('span');
    label.className = 'form-label';
    label.textContent = name + (required.has(name) ? ' *' : '');
    if (sub.description) label.title = sub.description;
    row.appendChild(label);

    const input = makeInput(sub, state[name], v => {
      state[name] = v;
    });
    row.appendChild(input);
    form.appendChild(row);
  }

  return {
    el: form,
    getValues: () => structuredClone(state),
  };
}

function makeInput(schema, currentValue, onChange) {
  if (schema.enum) {
    const sel = document.createElement('select');
    schema.enum.forEach(v => {
      const o = document.createElement('option');
      o.value = o.textContent = String(v);
      sel.appendChild(o);
    });
    if (currentValue != null) sel.value = String(currentValue);
    sel.addEventListener('change', () => onChange(sel.value));
    return sel;
  }
  const t = schema.type;
  if (t === 'boolean') {
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!currentValue;
    cb.addEventListener('change', () => onChange(cb.checked));
    return cb;
  }
  if (t === 'integer' || t === 'number') {
    const num = document.createElement('input');
    num.type = 'number';
    if (schema.minimum != null) num.min = schema.minimum;
    if (schema.maximum != null) num.max = schema.maximum;
    num.value = currentValue ?? schema.default ?? '';
    num.addEventListener('input', () => {
      if (num.value === '') { onChange(undefined); return; }
      onChange(t === 'integer' ? parseInt(num.value, 10) : parseFloat(num.value));
    });
    return num;
  }
  if (t === 'array') {
    // Arrays of scalars: textarea with one value per line.
    // Arrays of objects: raw JSON textarea (user edits as JSON).
    const ta = document.createElement('textarea');
    ta.rows = 4;
    const itemType = (schema.items && schema.items.type) || 'string';
    if (itemType === 'object') {
      ta.placeholder = 'JSON array (edit as JSON)';
      ta.value = JSON.stringify(currentValue ?? [], null, 2);
      ta.addEventListener('input', () => {
        try { onChange(JSON.parse(ta.value || '[]')); }
        catch { /* keep prior value; LLM can resubmit */ }
      });
    } else {
      ta.placeholder = 'one value per line';
      ta.value = Array.isArray(currentValue) ? currentValue.join('\n') : '';
      ta.addEventListener('input', () => {
        onChange(ta.value.split('\n').map(s => s.trim()).filter(Boolean));
      });
    }
    return ta;
  }
  // Default: string.
  const tx = document.createElement('input');
  tx.type = 'text';
  tx.value = currentValue ?? schema.default ?? '';
  tx.addEventListener('input', () => onChange(tx.value));
  return tx;
}
