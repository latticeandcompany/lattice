// The pin is read from a file this package does not own and may not understand.
//
// So the cases that matter are the malformed ones: a lattice.json from a newer
// schema, one that is not valid JSON at all, one with no pin. None of them may throw
// — a warning is not worth failing a command over.

import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';

import { driftWarning, findConfig, parsePin } from '../src/pin.ts';

const scratch = mkdtempSync(join(tmpdir(), 'lattice-npm-'));
after(() => rmSync(scratch, { recursive: true, force: true }));

test('the pin and the opt-out are read out of the config', () => {
	assert.deepEqual(parsePin('{"latticeVersion": "1.2.3"}'), {
		version: '1.2.3',
		versionCheck: true,
	});
	assert.deepEqual(
		parsePin('{"latticeVersion": "1.2.3", "settings": {"versionCheck": false}}'),
		{ version: '1.2.3', versionCheck: false },
	);
});

test('a config this package cannot read is not a reason to fail', () => {
	for (const text of ['', 'not json', '[]', 'null', '{}', '{"latticeVersion": 3}']) {
		const pin = parsePin(text);
		assert.equal(pin.version, null, JSON.stringify(text));
		assert.equal(pin.versionCheck, true, JSON.stringify(text));
	}
});

test('unknown keys and a newer schema are ignored, not rejected', () => {
	const pin = parsePin('{"latticeVersion": "9.9.9", "somethingFromTheFuture": {"a": 1}}');
	assert.equal(pin.version, '9.9.9');
});

test('the warning fires only on a real mismatch', () => {
	assert.equal(driftWarning(null, '1.0.0'), null);
	assert.equal(driftWarning('1.0.0', '1.0.0'), null);

	const warning = driftWarning('1.0.0', '2.0.0');
	assert.ok(warning?.includes('1.0.0'));
	assert.ok(warning?.includes('2.0.0'));
	assert.ok(warning?.includes('versionCheck'));
});

test('the nearest config wins, and a tree with none returns null', () => {
	const root = join(scratch, 'repo');
	const nested = join(root, 'apps', 'web');
	mkdirSync(nested, { recursive: true });
	writeFileSync(join(root, 'lattice.json'), '{"latticeVersion": "1.0.0"}');

	assert.equal(findConfig(nested), join(root, 'lattice.json'));
	assert.equal(findConfig(root), join(root, 'lattice.json'));

	writeFileSync(join(nested, 'lattice.json'), '{"latticeVersion": "2.0.0"}');
	assert.equal(findConfig(nested), join(nested, 'lattice.json'));

	const orphan = join(scratch, 'orphan');
	mkdirSync(orphan, { recursive: true });
	assert.equal(findConfig(orphan), null);
});
