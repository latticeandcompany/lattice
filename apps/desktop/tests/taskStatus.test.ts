import assert from 'node:assert/strict';
import { test } from 'node:test';
import { describeMiss, isBusy, opensOnFailure, statusView } from '../src/lib/taskStatus.ts';

test('every state carries a word, not only an icon', () => {
	const states = [
		'idle',
		'queued',
		'running',
		'cached',
		'done',
		'failed',
		'skipped',
		'exited',
	] as const;
	for (const state of states) {
		const view = statusView({ state }, 'web:build');
		assert.ok(view.label.length > 0, `${state} has no label`);
	}
});

test('a cache hit shows the short key beside the word', () => {
	const view = statusView({ state: 'cached', cacheKey: '664f4cd3aaaa' }, 'web:build');
	assert.equal(view.label, 'cache hit [664f4cd3]');
	assert.equal(view.icon, 'bi-lightning-charge');
});

test('a finished task shows how long it took', () => {
	assert.equal(statusView({ state: 'done', durationMs: 210 }, 'web:build').label, 'done (0.21s)');
});

test('a signal death is reported as such rather than as code null', () => {
	assert.equal(
		statusView({ state: 'exited', exitCode: null }, 'web:dev').label,
		'exited (killed by signal)',
	);
	assert.equal(statusView({ state: 'exited', exitCode: 1 }, 'web:dev').label, 'exited (code 1)');
});

// `dependency failed` is the only reason the runner produces.
test('a skipped task says why', () => {
	const view = statusView({ state: 'skipped', reason: 'dependency failed' }, 'web:build');
	assert.equal(view.label, 'skipped (dependency failed)');
});

test('running uses the spinner component rather than a glyph', () => {
	assert.equal(statusView({ state: 'running' }, 'web:build').icon, '');
});

test('only an in-flight task is busy', () => {
	assert.ok(isBusy('queued'));
	assert.ok(isBusy('running'));
	assert.ok(!isBusy('done'));
	assert.ok(!isBusy('idle'));
});

test('a failure opens its own output pane', () => {
	assert.ok(opensOnFailure('failed'));
	assert.ok(opensOnFailure('exited'));
	assert.ok(!opensOnFailure('done'));
});

// The same three sentences CacheMiss::describe produces in Rust, so the window and
// the terminal say the same thing about the same miss.
test('a miss reads the way the CLI prints it', () => {
	assert.equal(describeMiss({ kind: 'firstRun' }), 'cache miss (nothing cached for this task yet)');
	assert.equal(
		describeMiss({ kind: 'entryEvicted' }),
		'cache miss (the entry for this key is no longer in the cache)',
	);
	assert.equal(
		describeMiss({ kind: 'changed', components: ['inputs', 'manifests'] }),
		'cache miss: inputs, manifests changed',
	);
});
