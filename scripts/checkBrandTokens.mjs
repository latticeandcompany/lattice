// The website and the desktop app must look like one product, and they get there by
// sharing a style layer that is copied rather than imported.
//
// Copied, because there is no npm workspace root to publish a package from: each
// app under apps/ is a self-contained project with its own lockfile. Extracting one
// would mean creating a root manifest, making the site a workspace member, and
// rewriting the docs workflow's cache key — a lot of moving parts to share about
// 350 lines of SCSS.
//
// The cost of copying is silent drift, and this is what stops it. Three files must be
// byte-identical, and two carry a deliberate difference each:
//
// _paths.scss — the site is served from a GitHub Pages subpath and the app from the
// webview root, and Sass cannot read the base URL from the environment, so the literal
// lives in each copy.
//
// bootstrap.scss — the site is the standalone brand, which BRAND.md §1 keeps
// monochrome, so its $primary is ink. The app is Lattice Desktop and spends its own
// product colour, so its $primary is crimson-500 and it sets $color-contrast-* so the
// label Bootstrap picks for a filled accent is brand ink or paper rather than pure
// #000 / #fff. Every other setting still has to match, which is why this compares the
// settings rather than the bytes.
//
// The app's accent also has to override --focus, which _tokens.scss sets to teal for
// the site. That override lives in apps/desktop/src/styles/_accent.scss precisely so
// the shared token file can stay byte-identical.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(root, 'apps/web/src/styles');
const desktop = join(root, 'apps/desktop/src/styles');

const identical = ['_tokens.scss', '_fonts.scss', 'tailwind.css'];
const divergent = ['$primary', '$color-contrast-dark', '$color-contrast-light'];

const read = (path) => readFileSync(path, 'utf8').replace(/\r\n/g, '\n');

const failures = [];

for (const name of identical) {
	const a = read(join(web, name));
	const b = read(join(desktop, name));
	if (a !== b) {
		failures.push(`${name} differs between apps/web and apps/desktop`);
	}
}

// `$name: value` on its own line, which is every Sass setting in these files. The
// trailing separator goes because a token file ends the line with `;` and an argument
// list ends it with `,`, and the value is the same either way.
const declarations = (text) => {
	const found = new Map();
	for (const line of text.split('\n')) {
		const match = /^\s*(\$[\w-]+):\s*(.*?)[,;]?\s*$/.exec(line);
		if (match) found.set(match[1], match[2]);
	}
	return found;
};

const tokens = declarations(read(join(web, '_tokens.scss')));
const settings = {
	web: declarations(read(join(web, 'bootstrap.scss'))),
	desktop: declarations(read(join(desktop, 'bootstrap.scss'))),
};

for (const name of new Set([...settings.web.keys(), ...settings.desktop.keys()])) {
	if (divergent.includes(name)) continue;
	if (settings.web.get(name) !== settings.desktop.get(name)) {
		failures.push(`bootstrap.scss sets a different ${name} in apps/web and apps/desktop`);
	}
}

// The divergence itself is pinned, so "the app's accent" cannot quietly become a
// colour that is not in the brand.
const expected = [
	['web', '$primary', tokens.get('$ink')],
	['desktop', '$primary', tokens.get('$crimson-500')],
	['desktop', '$color-contrast-dark', tokens.get('$ink')],
	['desktop', '$color-contrast-light', tokens.get('$paper')],
];

for (const [surface, name, value] of expected) {
	if (settings[surface].get(name) !== value) {
		failures.push(
			`apps/${surface === 'web' ? 'web' : 'desktop'}'s bootstrap.scss must set ${name} to ${value}, not ${settings[surface].get(name)}`,
		);
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

console.log(
	`brand tokens agree (${identical.length} files identical, $base and the accent differ as designed)`,
);
