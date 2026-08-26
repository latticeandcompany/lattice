// Every row of the platform table, checked against the targets release.yml builds.
//
// The point of these is that the two lists cannot drift apart silently: if a target
// is added to or dropped from the release matrix, the triples asserted here are what
// a reviewer has to come and change.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { ALL_TARGETS, resolveTarget } from '../src/target.ts';

const GLIBC = true;
const MUSL = false;

test('each published platform resolves to its release triple', () => {
	const cases: [NodeJS.Platform, string, boolean, string, string][] = [
		['darwin', 'arm64', GLIBC, 'lattice-darwin-arm64', 'aarch64-apple-darwin'],
		['darwin', 'x64', GLIBC, 'lattice-darwin-x64', 'x86_64-apple-darwin'],
		['linux', 'x64', GLIBC, 'lattice-linux-x64-gnu', 'x86_64-unknown-linux-gnu'],
		['linux', 'x64', MUSL, 'lattice-linux-x64-musl', 'x86_64-unknown-linux-musl'],
		['linux', 'arm64', GLIBC, 'lattice-linux-arm64-gnu', 'aarch64-unknown-linux-gnu'],
		['win32', 'x64', GLIBC, 'lattice-win32-x64-msvc', 'x86_64-pc-windows-msvc'],
	];

	for (const [platform, arch, glibc, pkg, triple] of cases) {
		const target = resolveTarget(platform, arch, glibc);
		assert.equal(target?.pkg, pkg, `${platform}-${arch}`);
		assert.equal(target?.triple, triple, `${platform}-${arch}`);
	}
});

test('only Windows names the executable with an extension', () => {
	assert.equal(resolveTarget('win32', 'x64', GLIBC)?.exe, 'lattice.exe');
	assert.equal(resolveTarget('darwin', 'arm64', GLIBC)?.exe, 'lattice');
	assert.equal(resolveTarget('linux', 'x64', MUSL)?.exe, 'lattice');
});

test('Windows on ARM is served the x64 build', () => {
	assert.deepEqual(resolveTarget('win32', 'arm64', GLIBC), resolveTarget('win32', 'x64', GLIBC));
});

test('aarch64 musl has nothing published, and says so rather than guessing', () => {
	assert.equal(resolveTarget('linux', 'arm64', MUSL), null);
});

test('an unpublished platform resolves to null', () => {
	assert.equal(resolveTarget('freebsd', 'x64', GLIBC), null);
	assert.equal(resolveTarget('darwin', 'ia32', GLIBC), null);
	assert.equal(resolveTarget('linux', 'riscv64', GLIBC), null);
});

test('every resolvable target is one the package depends on', () => {
	const declared = new Set(ALL_TARGETS.map((target) => target.pkg));
	const reachable: [NodeJS.Platform, string, boolean][] = [
		['darwin', 'arm64', GLIBC],
		['darwin', 'x64', GLIBC],
		['linux', 'x64', GLIBC],
		['linux', 'x64', MUSL],
		['linux', 'arm64', GLIBC],
		['win32', 'x64', GLIBC],
		['win32', 'arm64', GLIBC],
	];

	for (const [platform, arch, glibc] of reachable) {
		const target = resolveTarget(platform, arch, glibc);
		assert.ok(target !== null, `${platform}-${arch} resolved to nothing`);
		assert.ok(declared.has(target.pkg), `${target.pkg} is not in ALL_TARGETS`);
	}
	assert.equal(declared.size, 6);
});
