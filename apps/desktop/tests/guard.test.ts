import assert from 'node:assert/strict';
import { test } from 'node:test';
import { guarded, message } from '../src/lib/guard.ts';

const recorder = () => {
	const busy: boolean[] = [];
	const errors: string[] = [];
	return {
		busy,
		errors,
		report: { onBusy: (next: boolean) => busy.push(next), onError: (e: string) => errors.push(e) },
	};
};

test('a command that worked says so', async () => {
	const seen = recorder();
	assert.equal(await guarded(async () => {}, seen.report), true);
	assert.deepEqual(seen.busy, [true, false]);
	assert.deepEqual(seen.errors, []);
});

// The editor's Save used to treat the resolved promise as proof: saving a read-only
// lattice.json showed the error and flipped the hint to "Saved" at the same time.
test('a command that failed reports the failure and returns false', async () => {
	const seen = recorder();
	const worked = await guarded(async () => {
		throw new Error('failed to write /repo/lattice.json: Permission denied');
	}, seen.report);

	assert.equal(worked, false);
	assert.deepEqual(seen.errors, ['failed to write /repo/lattice.json: Permission denied']);
	assert.deepEqual(seen.busy, [true, false], 'busy is cleared either way');
});

test('a rejection that is not an Error still reads as a sentence', async () => {
	const seen = recorder();
	await guarded(async () => {
		throw 'a run is already in progress';
	}, seen.report);
	assert.deepEqual(seen.errors, ['a run is already in progress']);
	assert.equal(message(new Error('boom')), 'boom');
});
