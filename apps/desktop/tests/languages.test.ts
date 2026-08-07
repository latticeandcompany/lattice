import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ART_SLUGS, DARK_VARIANT_SLUGS, hasDarkVariant, languageMark } from '../src/lib/languages.ts';

// The driver-to-ecosystem map lives in Rust so a new driver cannot skip it. What this
// checks is the second half: that every ecosystem resolves to *some* mark.
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

test('every ecosystem gets a mark, with or without artwork', () => {
	for (const language of ECOSYSTEMS) {
		const mark = languageMark('sometool', language);
		assert.ok(mark.kind === 'art' || mark.kind === 'monogram', `${language} has no mark`);
		if (mark.kind === 'monogram') {
			assert.ok(mark.monogram && mark.monogram.length > 0, `${language} has an empty monogram`);
		}
	}
});

test('the ecosystems we ship art for use it', () => {
	for (const language of ART_SLUGS) {
		const mark = languageMark('sometool', language);
		assert.equal(mark.kind, 'art');
		assert.equal(mark.slug, language);
	}
});

test('every art slug is a real ecosystem', () => {
	for (const slug of ART_SLUGS) {
		assert.ok(ECOSYSTEMS.includes(slug), `${slug} is not an ecosystem the engine reports`);
	}
});

test('node has a dark variant because its wordmark is ink', () => {
	assert.ok(hasDarkVariant('node'));
	assert.ok(!hasDarkVariant('go'));
	for (const slug of DARK_VARIANT_SLUGS) {
		assert.ok((ART_SLUGS as readonly string[]).includes(slug));
	}
});

test('a monogram comes from the ecosystem, not the tool that found it', () => {
	// `cargo` showing "Ca" would be wrong; it is Rust.
	const mark = languageMark('cargo', 'rust');
	assert.equal(mark.kind, 'monogram');
	assert.equal(mark.monogram, 'Rs');
});

test('an agnostic task runner has a mark but no ecosystem', () => {
	const mark = languageMark('just', null);
	assert.equal(mark.kind, 'monogram');
	assert.equal(mark.title, 'just');
});

test('no driver at all is its own state', () => {
	const mark = languageMark(null, null);
	assert.equal(mark.kind, 'none');
	assert.ok(mark.title.length > 0);
});
