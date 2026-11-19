import assert from 'node:assert/strict';
import { test } from 'node:test';

import { isPathKey, overrideCommand, overrideFacets } from '../src/lib/overrides.ts';

test('a key with a slash or a glob character matches paths, the way the Rust side reads it', () => {
	assert.equal(isPathKey('web'), false);
	assert.equal(isPathKey('@acme/ui'), true);
	assert.equal(isPathKey('apps/*'), true);
	assert.equal(isPathKey('.'), true);
	assert.equal(isPathKey('svc?'), true);
});

test('a task is a command string or an object carrying run', () => {
	assert.equal(overrideCommand('make'), 'make');
	assert.equal(overrideCommand({ run: 'vite build', outputs: ['dist/**'] }), 'vite build');
	assert.equal(overrideCommand({ cache: false }), undefined);
});

test('the summary names only what an override sets', () => {
	assert.deepEqual(overrideFacets({}), []);
	assert.deepEqual(
		overrideFacets({ driver: 'none', dependsOn: ['api'], tasks: { build: 'make' } }),
		['no driver', 'after api', '1 task'],
	);
});
