import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { computeMd5Hex, computeSha1Hex, computeSha256Hex } from './hashes.ts';

test('hashes: matches RFC 1321 MD5 and FIPS 180-4 SHA-1 / SHA-256 test vectors', () => {
  const empty = new Uint8Array(0);
  assert.strictEqual(computeMd5Hex(empty), 'd41d8cd98f00b204e9800998ecf8427e');
  assert.strictEqual(computeSha1Hex(empty), 'da39a3ee5e6b4b0d3255bfef95601890afd80709');
  assert.strictEqual(
    computeSha256Hex(empty),
    'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
  );

  const abc = new TextEncoder().encode('abc');
  assert.strictEqual(computeMd5Hex(abc), '900150983cd24fb0d6963f7d28e17f72');
  assert.strictEqual(computeSha1Hex(abc), 'a9993e364706816aba3e25717850c26c9cd0d89d');
  assert.strictEqual(
    computeSha256Hex(abc),
    'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
  );
});

test('hashes: matches node:crypto across multi-block binary buffer', () => {
  const buf = new Uint8Array(513);
  for (let i = 0; i < buf.length; i++) buf[i] = (i * 37 + 11) & 0xff;
  assert.strictEqual(computeMd5Hex(buf), crypto.createHash('md5').update(buf).digest('hex'));
  assert.strictEqual(computeSha1Hex(buf), crypto.createHash('sha1').update(buf).digest('hex'));
  assert.strictEqual(computeSha256Hex(buf), crypto.createHash('sha256').update(buf).digest('hex'));
});
