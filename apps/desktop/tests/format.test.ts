import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fmtSecs, isFullCache, runSummary, shortKey, shortenPath } from '../src/lib/format.ts';

// Matched against the Rust fmt_secs, so the window and the terminal never disagree
// about how long something took.
test('durations read the way the CLI prints them', () => {
	assert.equal(fmtSecs(210), '0.21s');
	assert.equal(fmtSecs(1230), '1.23s');
	assert.equal(fmtSecs(10), '0.01s');
	assert.equal(fmtSecs(59_999), '60.00s');
	assert.equal(fmtSecs(60_000), '1:00');
	assert.equal(fmtSecs(247_000), '4:07');
	assert.equal(fmtSecs(3_600_000), '1:00:00');
	assert.equal(fmtSecs(4_350_000), '1:12:30');
});

test('a key is shortened to the leading chunk', () => {
	assert.equal(shortKey('664f4cd3aaaaaaaaaaaa'), '664f4cd3');
	assert.equal(shortKey('abc'), 'abc');
});

test('the summary line matches the CLI, including the singular', () => {
	assert.equal(
		runSummary({ total: 4, cached: 1, failed: 0, elapsedMs: 390 }),
		'4 tasks, 1 cached, 0 failed, 0.39s',
	);
	assert.equal(
		runSummary({ total: 1, cached: 0, failed: 0, elapsedMs: 10 }),
		'1 task, 0 cached, 0 failed, 0.01s',
	);
});

test('a full cache needs every task cached and none failed', () => {
	assert.ok(isFullCache({ total: 3, cached: 3, failed: 0 }));
	assert.ok(!isFullCache({ total: 3, cached: 2, failed: 0 }));
	assert.ok(!isFullCache({ total: 3, cached: 3, failed: 1 }));
	// A run that scheduled nothing is not a full cache.
	assert.ok(!isFullCache({ total: 0, cached: 0, failed: 0 }));
});

test('a long path keeps its tail, which is the part that identifies it', () => {
	assert.equal(shortenPath('/Users/me/src/lattice'), '…/me/src/lattice');
	assert.equal(shortenPath('/a/b'), '/a/b');
});
