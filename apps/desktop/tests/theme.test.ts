import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MODES, resolve } from '../src/lib/theme.ts';

test('every combination of mode and OS preference resolves', () => {
	assert.equal(resolve('light', false), 'light');
	assert.equal(resolve('light', true), 'light');
	assert.equal(resolve('dark', false), 'dark');
	assert.equal(resolve('dark', true), 'dark');
	assert.equal(resolve('system', false), 'light');
	assert.equal(resolve('system', true), 'dark');
});

test('the three modes are offered with the icons the site uses', () => {
	assert.deepEqual(
		MODES.map((m) => m.mode),
		['light', 'dark', 'system'],
	);
	assert.deepEqual(
		MODES.map((m) => m.icon),
		['bi-sun', 'bi-moon-stars', 'bi-circle-half'],
	);
});
