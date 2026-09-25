import assert from 'node:assert/strict';
import { test } from 'node:test';

import { declaredGroups, engineSource, nameList } from '../src/lib/toolchains.ts';
import type { WorkspaceView } from '../src/lib/types.ts';

const ws = (name: string, engines: WorkspaceView['engines']): WorkspaceView => ({
	name,
	path: name,
	dependsOn: [],
	inferredDeps: [],
	driver: null,
	engines,
	tasks: [],
	problems: [],
});

const node = (version: string, installCmd: string | null = null) => ({
	name: 'node',
	version,
	versionCmd: null,
	installCmd,
	bin: null,
	wellKnown: true,
});

test('workspaces that pin the same engines share a group', () => {
	const groups = declaredGroups([
		ws('web', [node('>=22')]),
		ws('docs', [node('>=22')]),
		ws('api', [node('>=20')]),
		ws('plain', []),
	]);
	assert.equal(groups.length, 2);
	assert.deepEqual(groups[0].workspaces, ['web', 'docs']);
	assert.equal(groups[0].engines[0].wanted, '>=22');
});

test('an engine with an installCmd is marked as one Lattice installs', () => {
	const [group] = declaredGroups([ws('web', [node('22.1.0', 'curl … | tar')])]);
	assert.equal(group.engines[0].installs, true);
	assert.equal(engineSource(group.engines[0], false), 'installs on first run');
});

test('a checked engine says where it came from', () => {
	const base = { name: 'node', wanted: '>=22', installs: false, version: '22.1.0', location: '/usr/bin/node', provisioned: false };
	assert.equal(engineSource(base, true), 'on PATH');
	assert.equal(engineSource({ ...base, provisioned: true }, true), 'installed by Lattice');
	assert.equal(engineSource({ ...base, version: null }, true), 'unchecked');
});

test('a list of names reads as a sentence and stops before it gets long', () => {
	assert.equal(nameList(['api']), 'api');
	assert.equal(nameList(['api', 'web']), 'api and web');
	assert.equal(nameList(['a', 'b', 'c', 'd', 'e']), 'a, b, c and 2 more');
});
