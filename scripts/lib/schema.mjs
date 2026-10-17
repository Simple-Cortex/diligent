/**
 * Minimal JSON Schema 2020-12 validator (documented subset).
 *
 * Supported validation keywords: type, required, properties,
 * additionalProperties, items, enum, const, pattern, minLength, maxLength,
 * minimum, maximum, minItems, maxItems, uniqueItems, format (date, date-time,
 * uri), anyOf, oneOf, allOf, not, if/then/else, $ref (same file only).
 * Annotation keywords are accepted and ignored. Any other keyword is an error
 * in checkSchema(): a keyword that is silently skipped would let invalid data
 * pass, so this validator refuses to guess.
 */

const ANNOTATIONS = new Set([
  '$schema', '$id', '$comment', '$defs', 'definitions', 'title', 'description',
  'default', 'examples', 'deprecated'
]);
const VALIDATION = new Set([
  'type', 'required', 'properties', 'additionalProperties', 'items', 'enum', 'const',
  'pattern', 'minLength', 'maxLength', 'minimum', 'maximum', 'minItems', 'maxItems',
  'uniqueItems', 'format', 'anyOf', 'oneOf', 'allOf', 'not', 'if', 'then', 'else', '$ref'
]);
const TYPES = new Set(['object', 'array', 'string', 'number', 'integer', 'boolean', 'null']);
const FORMATS = new Set(['date', 'date-time', 'uri']);

export const SUPPORTED_KEYWORDS = Object.freeze([...VALIDATION].sort());

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function typeOf(value) {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  if (typeof value === 'number') return Number.isInteger(value) ? 'integer' : 'number';
  return typeof value;
}

function matchesType(value, type) {
  const actual = typeOf(value);
  if (type === 'number') return (actual === 'number' || actual === 'integer') && Number.isFinite(value);
  return actual === type;
}

export function isDate(value) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T00:00:00Z`);
  return !Number.isNaN(parsed.getTime()) && parsed.toISOString().startsWith(value);
}

export function isDateTime(value) {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})$/.exec(value);
  if (!match || !isDate(match[1])) return false;
  const [hour, minute, second] = [Number(match[2]), Number(match[3]), Number(match[4])];
  return hour <= 23 && minute <= 59 && second <= 60;
}

function isUri(value) {
  if (typeof value !== 'string' || value.length === 0) return false;
  try { new URL(value); return true; } catch { return false; }
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null) return false;
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((item, i) => deepEqual(item, b[i]));
  if (isPlainObject(a)) {
    if (!isPlainObject(b)) return false;
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every(key => deepEqual(a[key], b[key]));
  }
  return false;
}

function pointer(path) {
  return path.length === 0 ? '/' : path.map(part => String(part).replace(/~/g, '~0').replace(/\//g, '~1')).map(p => `/${p}`).join('');
}

function resolveRef(root, ref) {
  if (ref === '#') return root;
  if (!ref.startsWith('#/')) return undefined;
  let node = root;
  for (const raw of ref.slice(2).split('/')) {
    const key = raw.replace(/~1/g, '/').replace(/~0/g, '~');
    if (!isPlainObject(node) && !Array.isArray(node)) return undefined;
    node = node[key];
    if (node === undefined) return undefined;
  }
  return node;
}

/** Returns a list of problems with the schema itself (empty means usable). */
export function checkSchema(schema, location = '#', root = schema, problems = []) {
  if (typeof schema === 'boolean') return problems;
  if (!isPlainObject(schema)) { problems.push(`${location}: schema must be an object or boolean`); return problems; }
  for (const key of Object.keys(schema)) {
    if (!ANNOTATIONS.has(key) && !VALIDATION.has(key)) problems.push(`${location}: unsupported keyword "${key}"`);
  }
  if ('type' in schema) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    for (const type of types) if (!TYPES.has(type)) problems.push(`${location}/type: unknown type "${type}"`);
  }
  if ('format' in schema && !FORMATS.has(schema.format)) problems.push(`${location}/format: unsupported format "${schema.format}"`);
  if ('pattern' in schema) {
    try { new RegExp(schema.pattern, 'u'); } catch (error) { problems.push(`${location}/pattern: ${error.message}`); }
  }
  if ('$ref' in schema && resolveRef(root, schema.$ref) === undefined) problems.push(`${location}/$ref: unresolved reference "${schema.$ref}"`);
  for (const key of ['properties', '$defs', 'definitions']) {
    if (isPlainObject(schema[key])) for (const [name, child] of Object.entries(schema[key])) checkSchema(child, `${location}/${key}/${name}`, root, problems);
  }
  for (const key of ['items', 'additionalProperties', 'not', 'if', 'then', 'else']) {
    if (key in schema && typeof schema[key] !== 'boolean') checkSchema(schema[key], `${location}/${key}`, root, problems);
  }
  for (const key of ['anyOf', 'oneOf', 'allOf']) {
    if (Array.isArray(schema[key])) schema[key].forEach((child, i) => checkSchema(child, `${location}/${key}/${i}`, root, problems));
  }
  return problems;
}

function run(schema, value, path, root, errors) {
  if (schema === true) return;
  if (schema === false) { errors.push({ path: pointer(path), message: 'no value is allowed here' }); return; }
  const fail = message => errors.push({ path: pointer(path), message });

  if ('$ref' in schema) {
    const target = resolveRef(root, schema.$ref);
    if (target === undefined) fail(`unresolved reference ${schema.$ref}`);
    else run(target, value, path, root, errors);
  }

  if ('type' in schema) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!types.some(type => matchesType(value, type))) {
      fail(`expected ${types.join(' or ')}, got ${typeOf(value)}`);
      return;
    }
  }
  if ('const' in schema && !deepEqual(value, schema.const)) fail(`must equal ${JSON.stringify(schema.const)}`);
  if ('enum' in schema && !schema.enum.some(option => deepEqual(value, option))) {
    fail(`must be one of ${schema.enum.map(option => JSON.stringify(option)).join(', ')}, got ${JSON.stringify(value)}`);
  }

  if (typeof value === 'string') {
    if ('minLength' in schema && [...value].length < schema.minLength) fail(`must be at least ${schema.minLength} characters`);
    if ('maxLength' in schema && [...value].length > schema.maxLength) fail(`must be at most ${schema.maxLength} characters`);
    if ('pattern' in schema && !new RegExp(schema.pattern, 'u').test(value)) fail(`must match pattern ${schema.pattern}`);
    if (schema.format === 'date' && !isDate(value)) fail('must be an ISO date (YYYY-MM-DD)');
    if (schema.format === 'date-time' && !isDateTime(value)) fail('must be an RFC 3339 date-time with a UTC offset');
    if (schema.format === 'uri' && !isUri(value)) fail('must be an absolute URI');
  }
  if (typeof value === 'number') {
    if ('minimum' in schema && value < schema.minimum) fail(`must be >= ${schema.minimum}`);
    if ('maximum' in schema && value > schema.maximum) fail(`must be <= ${schema.maximum}`);
  }
  if (Array.isArray(value)) {
    if ('minItems' in schema && value.length < schema.minItems) fail(`must have at least ${schema.minItems} items`);
    if ('maxItems' in schema && value.length > schema.maxItems) fail(`must have at most ${schema.maxItems} items`);
    if (schema.uniqueItems === true) {
      for (let i = 0; i < value.length; i++) {
        for (let j = i + 1; j < value.length; j++) if (deepEqual(value[i], value[j])) { fail(`items ${i} and ${j} are duplicates`); i = value.length; break; }
      }
    }
    if ('items' in schema) value.forEach((item, i) => run(schema.items, item, [...path, i], root, errors));
  }
  if (isPlainObject(value)) {
    for (const key of schema.required || []) if (!(key in value)) errors.push({ path: pointer([...path, key]), message: 'required field is missing' });
    const properties = schema.properties || {};
    for (const [key, child] of Object.entries(value)) {
      if (key in properties) run(properties[key], child, [...path, key], root, errors);
      else if (schema.additionalProperties === false) errors.push({ path: pointer([...path, key]), message: 'unknown field (move it under "extensions" or remove it)' });
      else if (isPlainObject(schema.additionalProperties)) run(schema.additionalProperties, child, [...path, key], root, errors);
    }
  }

  if (Array.isArray(schema.allOf)) for (const child of schema.allOf) run(child, value, path, root, errors);
  if ('not' in schema) {
    const sub = [];
    run(schema.not, value, path, root, sub);
    if (sub.length === 0) fail('must not match the excluded shape');
  }
  if (Array.isArray(schema.anyOf)) {
    const branches = schema.anyOf.map(child => { const sub = []; run(child, value, path, root, sub); return sub; });
    if (!branches.some(sub => sub.length === 0)) {
      const best = branches.reduce((a, b) => (b.length < a.length ? b : a));
      fail(`does not match any allowed shape; closest: ${best.slice(0, 3).map(e => `${e.path} ${e.message}`).join('; ')}`);
    }
  }
  if (Array.isArray(schema.oneOf)) {
    const branches = schema.oneOf.map(child => { const sub = []; run(child, value, path, root, sub); return sub; });
    const matched = branches.filter(sub => sub.length === 0).length;
    if (matched !== 1) {
      const best = branches.reduce((a, b) => (b.length < a.length ? b : a));
      fail(matched === 0
        ? `does not match any allowed shape; closest: ${best.slice(0, 3).map(e => `${e.path} ${e.message}`).join('; ')}`
        : `matches ${matched} shapes but must match exactly one`);
    }
  }
  if ('if' in schema) {
    const sub = [];
    run(schema.if, value, path, root, sub);
    if (sub.length === 0) { if ('then' in schema) run(schema.then, value, path, root, errors); }
    else if ('else' in schema) run(schema.else, value, path, root, errors);
  }
}

/**
 * Validate a value. Throws if the schema itself uses unsupported keywords, so
 * unsupported constraints can never be skipped without notice.
 * @returns {{ ok: boolean, errors: { path: string, message: string }[] }}
 */
export function validate(schema, value) {
  const problems = checkSchema(schema);
  if (problems.length) throw new Error(`Invalid schema: ${problems.join('; ')}`);
  const errors = [];
  run(schema, value, [], schema, errors);
  return { ok: errors.length === 0, errors };
}
