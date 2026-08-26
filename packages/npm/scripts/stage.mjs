// Assemble the seven packages a release publishes, in the order they must go out.
//
// Nothing here touches the working tree. The wrapper is copied into a staging
// directory rather than published from packages/npm, so a publish cannot leave the
// repo modified and a half-finished one leaves nothing behind at all.
//
// Two things are added to the wrapper's manifest that the committed one does not
// carry: the version, and the six optionalDependencies. Those cannot be committed —
// they name packages at a version that does not exist on the registry until this
// script's output is published, and `npm ci` refuses to run against a lockfile that
// cannot resolve them. They are generated from the same table the runtime resolves
// against, so the list npm installs and the list the binary is looked up in are one
// list.
//
// This is a Node script rather than another shell one for that same reason: the
// table lives in src/target.ts, and writing it out a second time in bash would be a
// second thing to keep right.
//
// Usage: node scripts/stage.mjs <version> <extracted-dir> <out-dir>
//
// <extracted-dir> holds one directory per target, named the way release.yml names
// its archives: lattice-<version>-<triple>/. Prints one staged directory per line,
// the wrapper last.

import { chmodSync, copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { ALL_TARGETS, SCOPE } from '../src/target.ts';

const [version, extracted, out] = process.argv.slice(2);

if (version === undefined || extracted === undefined || out === undefined) {
	process.stderr.write('usage: node scripts/stage.mjs <version> <extracted-dir> <out-dir>\n');
	process.exit(2);
}

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(pkgDir, '..', '..');
const license = join(root, 'LICENSE');

const json = (value) => `${JSON.stringify(value, null, '\t')}\n`;

// --- the six platform packages ----------------------------------------------
// No `main` and no `exports`, so the wrapper can resolve package.json and walk to
// bin/ from there. `preferUnplugged` is for Yarn's zip-backed installs: an
// executable inside a zip cannot be executed.
const platformManifest = (target) => ({
	name: `${SCOPE}/${target.pkg}`,
	version,
	description: `The ${target.triple} binary for Lattice.`,
	license: 'ISC',
	author: 'Ryan Mullin',
	homepage: 'https://latticeandcompany.github.io/lattice',
	repository: {
		type: 'git',
		url: 'git+https://github.com/latticeandcompany/lattice.git',
		directory: 'packages/npm',
	},
	os: [target.os],
	cpu: [target.cpu],
	...(target.libc === null ? {} : { libc: [target.libc] }),
	files: ['bin'],
	preferUnplugged: true,
});

const platformReadme = (target) =>
	`# ${SCOPE}/${target.pkg}\n\n` +
	`The \`${target.triple}\` build of the [Lattice](https://latticeandcompany.github.io/lattice) CLI.\n\n` +
	'This package is one of six platform builds and contains nothing but the binary.\n' +
	`Do not depend on it directly — install [\`${SCOPE}/lattice\`](https://www.npmjs.com/package/${SCOPE}/lattice),\n` +
	'which depends on all six and picks the one your machine can run.\n';

for (const target of ALL_TARGETS) {
	const dir = join(out, target.pkg);
	mkdirSync(join(dir, 'bin'), { recursive: true });

	const exe = join(dir, 'bin', target.exe);
	copyFileSync(join(extracted, `lattice-${version}-${target.triple}`, target.exe), exe);
	// npm records the mode from the tarball, and a binary that arrives 644 cannot be
	// run. The archives are built on Windows too, where the source mode means nothing.
	chmodSync(exe, 0o755);

	copyFileSync(license, join(dir, 'LICENSE'));
	writeFileSync(join(dir, 'package.json'), json(platformManifest(target)));
	writeFileSync(join(dir, 'README.md'), platformReadme(target));

	process.stdout.write(`${dir}\n`);
}

// --- the wrapper, last ------------------------------------------------------
// Published only once every binary it points at is already resolvable. The other
// order leaves it installable and broken for as long as the rest take to upload.
const wrapper = join(out, 'lattice');
mkdirSync(join(wrapper, 'dist'), { recursive: true });

const manifest = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8'));
delete manifest.devDependencies;
delete manifest.scripts;

writeFileSync(
	join(wrapper, 'package.json'),
	json({
		...manifest,
		version,
		optionalDependencies: Object.fromEntries(
			ALL_TARGETS.map((target) => [`${SCOPE}/${target.pkg}`, version]),
		),
	}),
);

// LICENSE is the repo's, copied in here rather than kept as a second copy under
// packages/npm that could drift or get committed.
for (const file of manifest.files) {
	copyFileSync(file === 'LICENSE' ? license : join(pkgDir, file), join(wrapper, file));
}

process.stdout.write(`${wrapper}\n`);
