import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MODES, applyStoredMode, resolve } from '../src/lib/theme.ts';

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

// A webview's worth of globals. The native frame cannot be reached from here — that
// call needs a running window — but which theme launch resolves and applies can.
const install = (choice: string | null, systemPrefersDark: boolean) => {
	const attributes = new Map<string, string>();
	let stored = choice;
	Object.assign(globalThis, {
		document: {
			documentElement: {
				setAttribute: (name: string, value: string) => attributes.set(name, value),
				getAttribute: (name: string) => attributes.get(name) ?? null,
			},
		},
		localStorage: {
			getItem: () => stored,
			setItem: (_: string, value: string) => {
				stored = value;
			},
		},
		window: { matchMedia: () => ({ matches: systemPrefersDark }) },
	});
	return attributes;
};

// index.html paints the webview before React mounts, but it cannot call Tauri, so
// startup has to apply the stored choice again from inside the app.
test('launching applies the theme that was chosen last time', async () => {
	const attributes = install('dark', false);
	assert.equal(await applyStoredMode(), 'dark');
	assert.equal(attributes.get('data-bs-theme'), 'dark');
});

test('with nothing chosen, launching follows the OS', async () => {
	const dark = install(null, true);
	assert.equal(await applyStoredMode(), 'dark');
	assert.equal(dark.get('data-bs-theme'), 'dark');

	const light = install('nonsense', false);
	assert.equal(await applyStoredMode(), 'light');
	assert.equal(light.get('data-bs-theme'), 'light');
});
