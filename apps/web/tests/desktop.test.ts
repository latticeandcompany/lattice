import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { downloadUrl, platforms } from '../src/lib/desktop.ts';

// The download page links assets by exact name through /releases/latest/download.
// If a name is wrong, GitHub answers 404 rather than falling back. No build-time
// check can catch that, because the assets do not exist until a tag is pushed. So
// these tests compare the names against the workflow that produces them.
const workflow = readFileSync(
	fileURLToPath(new URL('../../../.github/workflows/release.yml', import.meta.url)),
	'utf8',
);

/** Rebuild the names release.yml stages, from the matrix and the staging step. */
const stagedByWorkflow = (): Set<string> => {
	const desktop = workflow.slice(workflow.indexOf('\n  desktop:'), workflow.indexOf('\n  publish:'));

	const rows = [...desktop.matchAll(/- name: (\S+)\n\s+os: \S+\n\s+target: \S+\n\s+bundles: (\S+)/g)];
	assert.ok(rows.length > 0, 'no matrix rows found in the desktop job');

	// The `stage "$f" <suffix>` lines are the one place a suffix is written. Read the
	// suffixes from there. Restating the mapping here would let the two drift apart.
	const suffixes = new Map(
		[...desktop.matchAll(/for f in "\$bundle"\/(\w+)\/\S+; do stage "\$f" (\S+); done/g)].map(
			([, kind, suffix]) => [kind, suffix],
		),
	);
	assert.ok(suffixes.size > 0, 'no stage lines found in the desktop job');

	const staged = new Set<string>();
	for (const [, name, bundles] of rows) {
		for (const bundle of bundles.split(',')) {
			const suffix = suffixes.get(bundle);
			assert.ok(suffix, `the desktop job builds "${bundle}" but never stages it`);
			staged.add(`lattice-desktop-${name}${suffix}`);
		}
	}
	return staged;
};

test('every advertised download is an asset the release workflow stages', () => {
	const staged = stagedByWorkflow();
	const advertised = platforms.flatMap((p) => p.downloads.map((d) => d.asset));

	for (const asset of advertised) {
		assert.ok(staged.has(asset), `the page links ${asset}, which no release job produces`);
	}
});

test('every asset the release workflow stages is advertised', () => {
	const advertised = new Set(platforms.flatMap((p) => p.downloads.map((d) => d.asset)));

	for (const asset of stagedByWorkflow()) {
		assert.ok(advertised.has(asset), `the release ships ${asset}, which the page never offers`);
	}
});

// Each matrix row asserts a bundle count, so a bundle that is skipped without
// failing the job still fails the release. That count has to agree with the number
// of bundles the row builds.
test('each matrix row expects as many bundles as it builds', () => {
	const desktop = workflow.slice(workflow.indexOf('\n  desktop:'), workflow.indexOf('\n  publish:'));
	const rows = [...desktop.matchAll(/- name: (\S+)\n[\s\S]*?bundles: (\S+)\n\s+expect: (\d+)/g)];
	assert.equal(rows.length, 5, 'expected five desktop matrix rows');

	for (const [, name, bundles, expect] of rows) {
		assert.equal(Number(expect), bundles.split(',').length, `${name} expects the wrong count`);
	}
});

test('downloads resolve to the latest release rather than a pinned tag', () => {
	const url = downloadUrl('lattice-desktop-macos-aarch64.dmg');
	assert.equal(
		url,
		'https://github.com/latticeandcompany/lattice/releases/latest/download/lattice-desktop-macos-aarch64.dmg',
	);
});
