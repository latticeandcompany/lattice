// The website and the desktop app must look like one product, and they get there by
// sharing a style layer that is copied rather than imported.
//
// Copied, because there is no npm workspace root to publish a package from: each
// app under apps/ is a self-contained project with its own lockfile. Extracting one
// would mean creating a root manifest, making the site a workspace member, and
// rewriting the docs workflow's cache key — a lot of moving parts to share about
// 350 lines of SCSS.
//
// The cost of copying is silent drift, and this is what stops it. Four files must be
// byte-identical. _paths.scss is the one deliberate difference: the site is served
// from a GitHub Pages subpath and the app from the webview root, and Sass cannot
// read the base URL from the environment, so the literal lives in each copy.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(root, 'apps/web/src/styles');
const desktop = join(root, 'apps/desktop/src/styles');

const identical = ['_tokens.scss', '_fonts.scss', 'bootstrap.scss', 'tailwind.css'];

const read = (path) => readFileSync(path, 'utf8').replace(/\r\n/g, '\n');

const failures = [];

for (const name of identical) {
	const a = read(join(web, name));
	const b = read(join(desktop, name));
	if (a !== b) {
		failures.push(`${name} differs between apps/web and apps/desktop`);
	}
}

// _paths.scss is allowed to differ, but only on the one line that carries the base.
const paths = {
	web: read(join(web, '_paths.scss')),
	desktop: read(join(desktop, '_paths.scss')),
};
const baseLine = (text) => text.split('\n').find((line) => line.startsWith('$base:'));

if (!baseLine(paths.web) || !baseLine(paths.desktop)) {
	failures.push('_paths.scss no longer declares $base in one of the two copies');
} else if (baseLine(paths.desktop) !== "$base: '';") {
	failures.push(`apps/desktop/src/styles/_paths.scss must set $base to '', not ${baseLine(paths.desktop)}`);
}

if (failures.length > 0) {
	console.error('brand token drift:');
	for (const failure of failures) console.error(`  - ${failure}`);
	console.error('\nEdit apps/web/src/styles and copy the file across, or the two surfaces stop matching.');
	process.exit(1);
}

console.log(`brand tokens agree (${identical.length} files identical, $base differs as designed)`);
