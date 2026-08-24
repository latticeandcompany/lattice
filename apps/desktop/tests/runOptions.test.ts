import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	CACHE_MODES,
	cacheFlags,
	defaultSelection,
	effectiveCache,
	effectiveSelection,
} from '../src/lib/runOptions.ts';

// This table is the one in `lattice run`: no_cache = no_cache || force, and
// no_store = no_cache. Getting it wrong means Force silently stops refreshing the
// entry it re-ran to refresh.
test('the three modes map onto the two flags the engine takes', () => {
	assert.deepEqual(cacheFlags('normal'), { noCache: false, force: false });
	assert.deepEqual(cacheFlags('force'), { noCache: false, force: true });
	assert.deepEqual(cacheFlags('ignore'), { noCache: true, force: false });
});

test('force re-runs but still writes; ignore writes nothing', () => {
	assert.deepEqual(effectiveCache(cacheFlags('normal')), { skipRead: false, skipWrite: false });
	assert.deepEqual(effectiveCache(cacheFlags('force')), { skipRead: true, skipWrite: false });
	assert.deepEqual(effectiveCache(cacheFlags('ignore')), { skipRead: true, skipWrite: true });
});

test('every mode is offered with a hint that says what differs', () => {
	assert.equal(CACHE_MODES.length, 3);
	for (const option of CACHE_MODES) {
		assert.ok(option.hint.length > 0, `${option.mode} has no hint`);
		assert.ok(option.hint.endsWith('.'), `${option.mode} hint is not a sentence`);
	}
});

test('a project starts on build when it has one, and on its first task when it does not', () => {
	assert.deepEqual(defaultSelection(['lint', 'build', 'test']), ['build']);
	assert.deepEqual(defaultSelection(['test', 'lint']), ['test']);
	assert.deepEqual(defaultSelection([]), []);
});

// The views holding a selection are not remounted across a project switch, so a
// stale pick used to reach the backend as an unknown task.
test('a selection the open project does not define is replaced', () => {
	assert.deepEqual(effectiveSelection(['build'], ['test', 'lint']), ['test']);
	assert.deepEqual(effectiveSelection(['build'], []), []);
});

test('what both projects have in common is kept', () => {
	assert.deepEqual(effectiveSelection(['build', 'docs'], ['build', 'test']), ['build']);
	assert.deepEqual(effectiveSelection(['test', 'lint'], ['lint', 'test', 'build']), ['test', 'lint']);
});
