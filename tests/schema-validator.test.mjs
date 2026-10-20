import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { SUPPORTED_KEYWORDS, checkSchema, isDate, isDateTime, validate } from '../scripts/lib/schema.mjs';
import { ROOT } from './helpers.mjs';

const ok = (schema, value) => assert.equal(validate(schema, value).ok, true, JSON.stringify(validate(schema, value).errors));
const bad = (schema, value, fragment) => {
  const result = validate(schema, value);
  assert.equal(result.ok, false, `expected ${JSON.stringify(value)} to be rejected`);
  if (fragment) assert.ok(result.errors.some(e => `${e.path} ${e.message}`.includes(fragment)), `no error containing "${fragment}" in ${JSON.stringify(result.errors)}`);
  return result;
};

test('type: each type accepts its value and rejects others', () => {
  const samples = { string: 's', number: 1.5, integer: 3, boolean: true, null: null, array: [], object: {} };
  for (const [type, value] of Object.entries(samples)) {
    ok({ type }, value);
    for (const [other, otherValue] of Object.entries(samples)) {
      if (other === type || (type === 'number' && other === 'integer')) continue;
      bad({ type }, otherValue);
    }
  }
  bad({ type: 'integer' }, 1.5);
  bad({ type: 'number' }, Number.NaN);
  bad({ type: 'number' }, Number.POSITIVE_INFINITY);
});

test('type arrays express nullable fields', () => {
  ok({ type: ['string', 'null'] }, null);
  ok({ type: ['string', 'null'] }, 'x');
  bad({ type: ['string', 'null'] }, 3, 'expected string or null');
});

test('required and properties report the failing path', () => {
  const schema = { type: 'object', required: ['a'], properties: { a: { type: 'string' }, b: { type: 'integer' } } };
  ok(schema, { a: 'x', b: 2 });
  bad(schema, { b: 2 }, '/a required field is missing');
  bad(schema, { a: 'x', b: 'no' }, '/b expected integer');
});

test('additionalProperties false rejects unknown fields; a schema validates them', () => {
  const closed = { type: 'object', properties: { a: {} }, additionalProperties: false };
  ok(closed, { a: 1 });
  bad(closed, { a: 1, typo: 2 }, '/typo unknown field');
  const typed = { type: 'object', additionalProperties: { type: 'string' } };
  ok(typed, { x: 'a', y: 'b' });
  bad(typed, { x: 'a', y: 2 }, '/y expected string');
});

test('items validates every element with an indexed path', () => {
  const schema = { type: 'array', items: { type: 'integer' } };
  ok(schema, [1, 2, 3]);
  bad(schema, [1, 'x'], '/1 expected integer');
});

test('enum and const', () => {
  ok({ enum: ['a', 'b', null] }, null);
  bad({ enum: ['a', 'b'] }, 'c', 'must be one of');
  ok({ const: { k: [1] } }, { k: [1] });
  bad({ const: 'x' }, 'y', 'must equal');
});

test('pattern, minLength and maxLength', () => {
  ok({ type: 'string', pattern: '^[a-z]+$' }, 'abc');
  bad({ type: 'string', pattern: '^[a-z]+$' }, 'ABC', 'must match pattern');
  ok({ type: 'string', minLength: 2 }, 'ab');
  bad({ type: 'string', minLength: 2 }, 'a', 'at least 2');
  bad({ type: 'string', maxLength: 2 }, 'abc', 'at most 2');
  ok({ pattern: '^a$' }, 5);
});

test('minimum, maximum, minItems, maxItems, uniqueItems', () => {
  ok({ type: 'number', minimum: 0, maximum: 10 }, 5);
  bad({ minimum: 0 }, -1, '>= 0');
  bad({ maximum: 10 }, 11, '<= 10');
  bad({ type: 'array', minItems: 1 }, [], 'at least 1');
  bad({ type: 'array', maxItems: 1 }, [1, 2], 'at most 1');
  ok({ type: 'array', uniqueItems: true }, [1, 2, { a: 1 }]);
  bad({ type: 'array', uniqueItems: true }, [1, { a: 1 }, { a: 1 }], 'duplicates');
});

test('format date, date-time and uri', () => {
  ok({ type: 'string', format: 'date' }, '2026-10-17');
  bad({ type: 'string', format: 'date' }, '2026-02-30', 'ISO date');
  bad({ type: 'string', format: 'date' }, '10/17/2026');
  ok({ type: 'string', format: 'date-time' }, '2026-10-17T12:00:00Z');
  ok({ type: 'string', format: 'date-time' }, '2026-10-17T12:00:00.250+05:00');
  bad({ type: 'string', format: 'date-time' }, '2026-10-17T12:00:00', 'UTC offset');
  bad({ type: 'string', format: 'date-time' }, '2026-10-17 12:00:00Z');
  ok({ type: 'string', format: 'uri' }, 'https://example.gov/a');
  bad({ type: 'string', format: 'uri' }, 'not a url', 'absolute URI');
  ok({ type: ['string', 'null'], format: 'date' }, null);
  assert.equal(isDate('2024-02-29'), true);
  assert.equal(isDate('2023-02-29'), false);
  assert.equal(isDateTime('2026-13-01T00:00:00Z'), false);
});

test('anyOf and oneOf', () => {
  const any = { anyOf: [{ type: 'string' }, { type: 'integer' }] };
  ok(any, 'a'); ok(any, 3);
  bad(any, true, 'does not match any allowed shape');
  const one = { oneOf: [{ type: 'integer' }, { type: 'number', minimum: 0 }] };
  ok(one, 0.5);
  bad(one, 2, 'matches 2 shapes');
  bad(one, 'x', 'does not match any allowed shape');
});

test('allOf, not and if/then/else', () => {
  ok({ allOf: [{ type: 'integer' }, { minimum: 3 }] }, 4);
  bad({ allOf: [{ type: 'integer' }, { minimum: 3 }] }, 2);
  ok({ not: { type: 'string' } }, 3);
  bad({ not: { type: 'string' } }, 'x', 'must not match');
  const conditional = {
    type: 'object',
    if: { properties: { kind: { const: 'a' } }, required: ['kind'] },
    then: { required: ['a'] },
    else: { required: ['b'] }
  };
  ok(conditional, { kind: 'a', a: 1 });
  ok(conditional, { kind: 'z', b: 1 });
  bad(conditional, { kind: 'a' }, '/a required');
  bad(conditional, { kind: 'z' }, '/b required');
});

test('$ref resolves within the same file, including recursion', () => {
  const schema = { $defs: { node: { type: 'object', properties: { next: { $ref: '#/$defs/node' }, id: { type: 'integer' } }, additionalProperties: false } }, $ref: '#/$defs/node' };
  ok(schema, { id: 1, next: { id: 2, next: { id: 3 } } });
  bad(schema, { id: 1, next: { id: 'x' } }, '/next/id expected integer');
  const broken = { $ref: '#/$defs/missing' };
  assert.throws(() => validate(broken, 1), /unresolved reference/);
});

test('an unsupported keyword or format is an error, never silently skipped', () => {
  assert.throws(() => validate({ type: 'string', minProperties: 1 }, 'x'), /unsupported keyword "minProperties"/);
  assert.throws(() => validate({ type: 'string', format: 'email' }, 'x'), /unsupported format/);
  assert.throws(() => validate({ type: 'wat' }, 'x'), /unknown type/);
  assert.ok(checkSchema({ patternProperties: {} }).length > 0);
  assert.ok(SUPPORTED_KEYWORDS.includes('if'));
});

test('boolean schemas', () => {
  ok(true, 1);
  bad(false, 1, 'no value is allowed');
  ok({ properties: { a: true } }, { a: 1 });
});

test('every shipped schema uses only supported keywords and has the contract header', async () => {
  const dir = join(ROOT, 'schemas');
  const names = (await readdir(dir)).filter(n => n.endsWith('.schema.json'));
  assert.equal(names.length, 34);
  for (const name of names) {
    const schema = JSON.parse(await readFile(join(dir, name), 'utf8'));
    assert.deepEqual(checkSchema(schema), [], name);
    assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema', name);
    assert.equal(schema.additionalProperties, false, `${name} must close the top level`);
    assert.ok(schema.required.includes('schema_version'), `${name} must require schema_version`);
    assert.ok(schema.properties.extensions, `${name} must allow an extensions object`);
  }
});
