import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { ART_SLUGS, languageMark } from '../src/lib/languages.ts';

// The driver-to-ecosystem map lives in Rust so a new driver cannot skip it. What this
// checks is the second half: that every ecosystem resolves to a mark.
const ECOSYSTEMS = [
	'node',
	'rust',
	'go',
	'python',
	'ruby',
	'java',
	'kotlin',
	'dotnet',
	'swift',
	'php',
	'elixir',
	'dart',
	'haskell',
];

test('every ecosystem the engine reports has artwork', () => {
	for (const language of ECOSYSTEMS) {
		const mark = languageMark('sometool', language);
		assert.equal(mark.kind, 'art', `${language} has no artwork`);
		assert.equal(mark.slug, language);
	}
});

test('the art list and the ecosystem list are the same set', () => {
	assert.deepEqual([...ART_SLUGS].sort(), [...ECOSYSTEMS].sort());
});

// `languageArt.ts` cannot be imported here — the test runner does not resolve an SVG
// import — so the directory is checked instead. A slug with no file would render an
// empty square, which nothing else would catch.
test('every slug has a file, and no file is stranded', () => {
	const directory = fileURLToPath(new URL('../src/assets/languages', import.meta.url));
	assert.deepEqual(readdirSync(directory).sort(), [...ART_SLUGS].sort().map((slug) => `${slug}.svg`));
});

test('an ecosystem this build has never heard of still gets a mark', () => {
	// A newer engine can report a driver whose language predates this window.
	const mark = languageMark('zig', 'zig');
	assert.equal(mark.kind, 'monogram');
	assert.equal(mark.monogram, 'Zi');
});

test('an agnostic task runner has a mark but no ecosystem', () => {
	const mark = languageMark('just', null);
	assert.equal(mark.kind, 'monogram');
	assert.equal(mark.monogram, 'Ju');
	assert.equal(mark.title, 'just');
});

test('no driver at all is its own state', () => {
	const mark = languageMark(null, null);
	assert.equal(mark.kind, 'none');
	assert.ok(mark.title.length > 0);
});
