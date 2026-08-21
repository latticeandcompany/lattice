import assert from 'node:assert/strict';
import { test } from 'node:test';
import { FALLBACK, readTokens } from '../src/lib/themeTokens.ts';

// A canvas cannot read CSS, so these values are read out of the document and handed
// over. With no document at all the brand defaults still apply, which is what keeps a
// chart from rendering invisible.
test('with no document the brand defaults are used', () => {
	const tokens = readTokens();
	assert.equal(tokens.surface, FALLBACK.surface);
	assert.equal(tokens.focus, FALLBACK.focus);
	assert.equal(tokens.dark, false);
});

test('the fallbacks are the brand values, never pure black or white', () => {
	assert.equal(FALLBACK.text, '#020d0c');
	assert.equal(FALLBACK.surface, '#fbf8ff');
	assert.notEqual(FALLBACK.text, '#000000');
	assert.notEqual(FALLBACK.surface, '#ffffff');
	assert.equal(FALLBACK.focus, '#ca2e55');
	assert.equal(FALLBACK.fail, '#b45309');
});
