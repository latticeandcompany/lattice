import assert from 'node:assert/strict';
import { test } from 'node:test';

import { rosetteProgress, STROKES } from '../src/lib/progress.ts';

test('with no run the mark is whole and quiet', () => {
	assert.deepEqual(rosetteProgress({ total: 0, settled: 0, failed: 0 }, false), { drawn: STROKES, tone: 'idle' });
});

test('a run that has settled nothing still draws one stroke', () => {
	assert.equal(rosetteProgress({ total: 10, settled: 0, failed: 0 }, true).drawn, 1);
});

test('each settled eighth draws a stroke', () => {
	const half = rosetteProgress({ total: 8, settled: 4, failed: 0 }, true);
	assert.deepEqual(half, { drawn: 4, tone: 'live' });
});

test('a failure changes the colour, not the count', () => {
	const failed = rosetteProgress({ total: 8, settled: 2, failed: 1 }, true);
	assert.equal(failed.tone, 'failed');
	assert.equal(failed.drawn, 2);
});

test('a finished run shows how far it got', () => {
	assert.deepEqual(rosetteProgress({ total: 4, settled: 4, failed: 0 }, false), { drawn: 8, tone: 'idle' });
});
