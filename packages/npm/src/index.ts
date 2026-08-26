// Finding the binary npm installed.
//
// The binary is not in this package. It lives in one of six sibling packages, each
// declaring the `os` and `cpu` it is for, and the published manifest lists all six
// as optionalDependencies — so a package manager unpacks only the one that matches
// and nobody downloads five binaries they cannot run. Nothing is fetched at install
// time and nothing runs a postinstall script, which is what makes this work behind a
// proxy, from a lockfile, offline, and under --ignore-scripts.
//
// Resolution goes through the sibling's package.json rather than straight to the
// executable. Node only guarantees a deep path inside a package is resolvable when
// that package has no `exports` field, while package.json is reachable either way,
// so this holds even if a platform package ever grows one.

import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

import { resolveTarget, SCOPE, type Target } from './target.ts';

declare const __LATTICE_VERSION__: string;

/** The version of the Lattice CLI this package carries. */
export const version: string = __LATTICE_VERSION__;

const req = createRequire(import.meta.url);

const unsupported = (): Error =>
	new Error(
		`Lattice does not publish a binary for ${process.platform}-${process.arch}.\n` +
			'Build it from source with `cargo install --git ' +
			'https://github.com/latticeandcompany/lattice lattice`, or see\n' +
			'https://latticeandcompany.github.io/lattice/docs/installation',
	);

const missing = (target: Target): Error =>
	new Error(
		`Lattice installed, but ${SCOPE}/${target.pkg} did not.\n\n` +
			'That package is an optional dependency, so a failed or skipped install is silent.\n' +
			'Things that cause it:\n' +
			'  - installing with --no-optional, or --omit=optional\n' +
			'  - a lockfile made on a different platform, with npm\n' +
			'    (https://github.com/npm/cli/issues/4828) — delete node_modules and\n' +
			'    package-lock.json, then install again\n' +
			'  - a registry mirror that does not carry the platform packages\n\n' +
			`The same binary ships as lattice-${version}-${target.triple}.tar.gz on\n` +
			'https://github.com/latticeandcompany/lattice/releases',
	);

/**
 * The absolute path to the `lattice` executable for this machine.
 *
 * Throws with the reason when there is none: either nothing is published for this
 * platform, or the package that should carry it was not installed.
 */
export const binaryPath = (): string => {
	const target = resolveTarget();
	if (target === null) throw unsupported();

	let manifest: string;
	try {
		manifest = req.resolve(`${SCOPE}/${target.pkg}/package.json`);
	} catch {
		throw missing(target);
	}

	const exe = join(dirname(manifest), 'bin', target.exe);
	if (!existsSync(exe)) throw missing(target);
	return exe;
};
