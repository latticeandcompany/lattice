import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	fmtSecs,
	fmtSpan,
	isFullCache,
	runSummary,
	shortKey,
	shortenPath,
} from '../src/lib/format.ts';

// Matched against the Rust fmt_secs, so the window and the terminal never disagree
// about how long something took.
test('durations read the way the CLI prints them', () => {
	assert.equal(fmtSecs(210), '0.21s');
	assert.equal(fmtSecs(1230), '1.23s');
	assert.equal(fmtSecs(10), '0.01s');
	assert.equal(fmtSecs(59_499), '59.50s');
	// The half-second rounds the branch, so this never reads as 60.00s or as :60.
	assert.equal(fmtSecs(59_999), '1:00');
	assert.equal(fmtSecs(60_000), '1:00');
	assert.equal(fmtSecs(119_500), '2:00');
	assert.equal(fmtSecs(247_000), '4:07');
	assert.equal(fmtSecs(3_599_500), '1:00:00');
	assert.equal(fmtSecs(3_600_000), '1:00:00');
	assert.equal(fmtSecs(4_350_000), '1:12:30');
});

test('a key is shortened to the leading chunk', () => {
	assert.equal(shortKey('664f4cd3aaaaaaaaaaaa'), '664f4cd3');
	assert.equal(shortKey('abc'), 'abc');
});

// `lattice-output` never singularizes `tasks`, so neither does this. Fix it there
// first, then here.
test('the summary line matches the CLI, plural and all', () => {
	assert.equal(
		runSummary({ total: 4, cached: 1, failed: 0, elapsedMs: 390, savedMs: 0 }),
		'4 tasks, 1 cached, 0 failed, 0.39s',
	);
	assert.equal(
		runSummary({ total: 1, cached: 0, failed: 0, elapsedMs: 10, savedMs: 0 }),
		'1 tasks, 0 cached, 0 failed, 0.01s',
	);
});

test('a run with hits reports the task time they skipped', () => {
	assert.equal(
		runSummary({ total: 4, cached: 3, failed: 0, elapsedMs: 390, savedMs: 171_000 }),
		'4 tasks, 3 cached, 0 failed, 0.39s, 2m 51s saved',
	);
});

// A task that finished inside a millisecond saved nothing measurable, and a
// trailing `0.00s saved` would be noise on every fast run.
test('nothing measurable saved says nothing', () => {
	assert.equal(
		runSummary({ total: 1, cached: 1, failed: 0, elapsedMs: 2, savedMs: 0 }),
		'1 tasks, 1 cached, 0 failed, 0.00s',
	);
});

test('a saved span reads as minutes and hours past a minute', () => {
	assert.equal(fmtSpan(4_266), '4.27s');
	assert.equal(fmtSpan(60_000), '1m 00s');
	assert.equal(fmtSpan(247_000), '4m 07s');
	assert.equal(fmtSpan(3_600_000), '1h 00m');
	assert.equal(fmtSpan(51_720_000), '14h 22m');
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
