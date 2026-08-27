import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	FIRST_BURST,
	LAG_FRAME_MS,
	budget,
	burstFor,
	launcherCount,
	nextCeiling,
	rawCount,
	totalParticles,
} from '../src/lib/confetti.ts';

test('every consecutive full-power run spends more than the last', () => {
	const counts = [1, 2, 3, 4, 5, 6].map(rawCount);
	for (let i = 1; i < counts.length; i += 1) {
		assert.ok(counts[i] > counts[i - 1], `streak ${i + 1} is not bigger than ${i}`);
	}
	assert.equal(counts[0], FIRST_BURST);
});

test('the escalation is the kind that gets out of hand', () => {
	// Not a taste judgement: a linear ramp would never reach a ceiling worth finding.
	assert.ok(rawCount(6) > rawCount(1) * 10);
});

test('launchers are brought in one at a time and then stop', () => {
	assert.equal(launcherCount(1), 1);
	assert.equal(launcherCount(4), 4);
	assert.equal(launcherCount(7), 7);
	assert.equal(launcherCount(40), 7);
});

test('a burst spends its whole budget across the launchers it has', () => {
	const shots = burstFor(5, null);
	assert.equal(shots.length, 5);
	// Rounding per launcher, so the total lands near the budget rather than on it.
	assert.ok(Math.abs(totalParticles(shots) - rawCount(5)) <= shots.length);
});

test('a later burst is bigger paper, not just more of it', () => {
	const early = burstFor(1, null)[0];
	const late = burstFor(7, null)[0];
	assert.ok(late.spread > early.spread);
	assert.ok(late.startVelocity > early.startVelocity);
	assert.ok(late.scalar > early.scalar);
	assert.ok(late.ticks > early.ticks);
});

test('a ceiling holds the budget down no matter how long the streak gets', () => {
	assert.equal(budget(20, 400), 400);
	assert.equal(budget(1, 400), FIRST_BURST);
	assert.equal(budget(20, null), rawCount(20));
});

test('a smooth burst sets no ceiling', () => {
	assert.equal(nextCeiling(null, 5000, LAG_FRAME_MS), null);
});

test('the burst that drags the window under sets the ceiling below itself', () => {
	const found = nextCeiling(null, 5000, LAG_FRAME_MS + 20);
	assert.ok(found !== null && found < 5000);
	assert.ok(found >= FIRST_BURST);
});

test('a ceiling once found is never raised again', () => {
	// Otherwise a good run would rediscover it by driving the window into the ground.
	assert.equal(nextCeiling(400, 5000, 4), 400);
	assert.equal(nextCeiling(400, 5000, 200), 400);
});
