import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	detectEol,
	detectIndent,
	insertInto,
	offsetOf,
	positionAt,
	removeAt,
	setValue,
} from '../src/lib/configEdit.ts';

const CONFIG = [
	'{',
	'\t"$schema": ".lattice/schema.json",',
	'\t"latticeVersion": "1.0.0",',
	'\t"workspaces": [',
	'\t\t{ "name": "web", "path": "apps/web" },',
	'\t\t{ "name": "api", "path": "services/api" }',
	'\t],',
	'\t"tasks": {',
	'\t\t"build": { "dependsOn": ["^build"], "outputs": ["dist/**"] }',
	'\t}',
	'}',
	'',
].join('\n');

// The acceptance test for the whole design: the editor holds text, not an object.
test('loading and saving without an edit changes nothing', () => {
	// Reading it and writing it back through the edit layer is a no-op.
	const same = setValue(CONFIG, ['latticeVersion'], '1.0.0');
	assert.equal(same, CONFIG);
});

// The other acceptance test. The Rust types are deny_unknown_fields, so a key this
// build has never heard of is still valid input the user is entitled to keep.
test('a key the app does not know survives an unrelated edit', () => {
	const withUnknown = CONFIG.replace(
		'"build": { "dependsOn": ["^build"], "outputs": ["dist/**"] }',
		'"build": { "dependsOn": ["^build"], "futureField": 42 }',
	);
	const edited = setValue(withUnknown, ['latticeVersion'], '2.0.0');
	assert.ok(edited.includes('"futureField": 42'), 'a parse-and-reserialize would have dropped it');
	assert.ok(edited.includes('"2.0.0"'));
});

test('key order is preserved when a value changes', () => {
	const edited = setValue(CONFIG, ['latticeVersion'], '9.9.9');
	const keys = [...edited.matchAll(/"(\$schema|latticeVersion|workspaces|tasks)"/g)].map(
		(match) => match[1],
	);
	assert.deepEqual(keys, ['$schema', 'latticeVersion', 'workspaces', 'tasks']);
});

test('CRLF files stay CRLF', () => {
	const crlf = CONFIG.replace(/\n/g, '\r\n');
	assert.equal(detectEol(crlf), '\r\n');
	const edited = setValue(crlf, ['settings', 'cacheDir'], '.cache');
	// No stray lone newline was introduced.
	assert.equal(edited.split('\n').length - 1, edited.split('\r\n').length - 1);
});

test('LF files stay LF', () => {
	assert.equal(detectEol(CONFIG), '\n');
	const edited = setValue(CONFIG, ['settings', 'cacheDir'], '.cache');
	assert.ok(!edited.includes('\r'));
});

test('indentation is taken from the file rather than assumed', () => {
	assert.deepEqual(detectIndent(CONFIG), { tabSize: 1, insertSpaces: false });
	const spaced = CONFIG.replace(/\t/g, '  ');
	assert.deepEqual(detectIndent(spaced), { tabSize: 2, insertSpaces: true });
});

test('a missing parent is created on the way to the value', () => {
	const edited = setValue(CONFIG, ['settings', 'cacheDir'], '.lattice/cache');
	assert.ok(edited.includes('"settings"'));
	assert.ok(edited.includes('"cacheDir": ".lattice/cache"'));
});

test('removing a member takes its comma with it', () => {
	const edited = removeAt(CONFIG, ['latticeVersion']);
	assert.ok(!edited.includes('latticeVersion'));
	assert.ok(!edited.includes(',,'));
	assert.doesNotThrow(() => JSON.parse(edited));
});

test('appending to an array leaves the existing entries alone', () => {
	const edited = insertInto(CONFIG, ['workspaces'], { name: 'docs', path: 'apps/docs' });
	const parsed = JSON.parse(edited);
	assert.equal(parsed.workspaces.length, 3);
	assert.deepEqual(parsed.workspaces[0], { name: 'web', path: 'apps/web' });
	assert.equal(parsed.workspaces[2].name, 'docs');
});

test('an offset points at the value, so a raw view can scroll to it', () => {
	const offset = offsetOf(CONFIG, ['workspaces', 1, 'name']);
	assert.ok(offset !== null);
	assert.ok(CONFIG.slice(offset as number).startsWith('"api"'));

	const where = positionAt(CONFIG, offset as number);
	assert.equal(where.line, 6);
});

test('an offset for something absent is null rather than zero', () => {
	assert.equal(offsetOf(CONFIG, ['nope']), null);
});

test('editing text that is not valid JSON yet does not throw', () => {
	assert.doesNotThrow(() => setValue('{ "a":', ['b'], 1));
});
