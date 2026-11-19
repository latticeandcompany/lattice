import assert from 'node:assert/strict';
import { test } from 'node:test';

import { lineCounts, lineDiff } from '../src/lib/lineDiff.ts';

test('an unchanged file is all context', () => {
	const diff = lineDiff('a\nb\n', 'a\nb\n');
	assert.deepEqual(
		diff.map((line) => line.kind),
		['same', 'same'],
	);
});

test('a migration that drops lines marks them removed and keeps the rest in order', () => {
	const before = '{\n  "workspaces": [\n    { "name": "a", "path": "a" }\n  ]\n}\n';
	const after = '{\n  "workspaces": ["a"]\n}\n';
	const diff = lineDiff(before, after);
	assert.equal(diff[0].kind, 'same');
	assert.equal(diff[diff.length - 1].text, '}');
	assert.ok(diff.some((line) => line.kind === 'added' && line.text.includes('["a"]')));
	assert.equal(diff.filter((line) => line.kind === 'removed').length, 3);
});

test('line endings do not count as changes', () => {
	const diff = lineDiff('a\r\nb\r\n', 'a\nb\n');
	assert.ok(diff.every((line) => line.kind === 'same'));
});

test('counts are lines, not a trailing empty one', () => {
	assert.deepEqual(lineCounts('a\nb\nc\n', 'a\n'), { before: 3, after: 1 });
});
