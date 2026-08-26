// The one list that has to agree with something outside this package.
//
// src/target.ts and the build matrix in release.yml describe the same six targets.
// If they ever disagree the failure is quiet and late: a target added to the matrix
// is simply never published to npm, and a target removed from it makes the publish
// script look for an archive that no release contains. Neither shows up until
// someone is mid-release, so it is checked here instead.
//
// The matrix is read with a regex rather than a YAML parser, which is worth being
// honest about: it is looking for `- target: <triple>` lines inside the `build:`
// job. That is the shape release.yml has, and the assertion below fails loudly if
// the file ever stops having it.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { ALL_TARGETS } from '../src/target.ts';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

test('every target release.yml builds has a platform package, and vice versa', () => {
	const workflow = readFileSync(join(root, '.github', 'workflows', 'release.yml'), 'utf8');

	// The CLI matrix only: `desktop:` declares targets of its own, and those ship as
	// installers rather than as npm packages.
	const start = workflow.indexOf('\n  build:');
	const end = workflow.indexOf('\n  desktop:');
	assert.ok(start !== -1 && end > start, 'release.yml no longer has a build job before desktop');

	const matrix = [...workflow.slice(start, end).matchAll(/^\s*-\s*target:\s*(\S+)\s*$/gm)].map(
		(match) => match[1],
	);
	assert.ok(matrix.length > 0, 'found no `- target:` lines in the build matrix');

	assert.deepEqual(
		[...matrix].sort(),
		ALL_TARGETS.map((target) => target.triple).sort(),
		'src/target.ts and the release.yml build matrix disagree',
	);
});
