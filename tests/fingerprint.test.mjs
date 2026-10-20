import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { extensionInstance, resultFingerprint } from '../scripts/lib/fingerprint.mjs';

const row = (extensions) => ({ check_id: 'BRW-TEXT-CONTRAST', instance_id: 'https://agency.example.gov/', extensions });
const sha = text => createHash('sha256').update(text).digest('hex');

test('rows that differ only in the extension instance get different fingerprints', () => {
  const a = row({ 'diligent.assess': { instance: 'main > p:nth-child(1)' } });
  const b = row({ 'diligent.assess': { instance: 'main > p:nth-child(2)' } });
  assert.notEqual(resultFingerprint(a), resultFingerprint(b));
  assert.equal(resultFingerprint(a), sha('BRW-TEXT-CONTRAST\0https://agency.example.gov/\0main > p:nth-child(1)\0BRW-TEXT-CONTRAST'));
});

test('the instance is read from any extension key, including source rows', () => {
  assert.equal(extensionInstance(row({ 'diligent.source': { instance: 'a.link' } })), 'a.link');
});

test('an infra row without an extension instance uses the empty string', () => {
  const infra = row({ 'diligent.infra': { scope: 'network', rule_ids: ['INFRA-02'] } });
  assert.equal(extensionInstance(infra), '');
  assert.equal(resultFingerprint(infra), sha('BRW-TEXT-CONTRAST\0https://agency.example.gov/\0\0INFRA-02'));
  assert.equal(resultFingerprint(row(undefined)), sha('BRW-TEXT-CONTRAST\0https://agency.example.gov/\0\0BRW-TEXT-CONTRAST'));
});

test('a non-string instance is ignored', () => {
  assert.equal(extensionInstance(row({ 'diligent.assess': { instance: 7 } })), '');
});

test('rows that differ only in obligation_id get different fingerprints', () => {
  const base = { check_id: 'BRW-FED-IDENTIFIER', instance_id: 'https://agency.example.gov/::footer', extensions: { 'diligent.assess': { instance: 'footer' } } };
  const a = { ...base, obligation_id: 'OBL-FED-04' };
  const b = { ...base, obligation_id: 'OBL-FED-05' };
  assert.notEqual(resultFingerprint(a), resultFingerprint(b));
});

test('a rule id from an extension wins over obligation_id', () => {
  const row = { check_id: 'C', instance_id: 'i', obligation_id: 'OBL-X', extensions: { 'diligent.assess': { rule_ids: ['FED-13'] } } };
  assert.equal(resultFingerprint(row), sha('C\0i\0\0FED-13'));
});
