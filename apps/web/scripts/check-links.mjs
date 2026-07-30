// Post-build link check. The site is served from a subpath, which is where both of
// the bugs this guards against came from: a link that forgets the base 404s, and one
// that applies it twice 404s the same way. Run against dist/ after `astro build` and
// `pagefind`.
import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

// Pagefind writes each fragment gzipped, and marks an already-decompressed payload
// with this prefix so its own loader can tell the two apart.
const marker = 'pagefind_dcd';
const decompress = (buffer) =>
	buffer.subarray(0, marker.length).toString('utf8') === marker ? buffer : gunzipSync(buffer);

const webRoot = fileURLToPath(new URL('..', import.meta.url));
const dist = join(webRoot, 'dist');
const config = readFileSync(join(webRoot, 'astro.config.mjs'), 'utf8');
const base = (config.match(/base:\s*['"]([^'"]+)['"]/)?.[1] ?? '').replace(/\/$/, '');

const failures = [];
const fail = (message) => failures.push(message);

const walk = (dir) =>
	readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);
		return entry.isDirectory() ? walk(path) : [path];
	});

if (!existsSync(dist)) {
	console.error('dist/ is missing — run `npm run build` first.');
	process.exit(1);
}

const files = walk(dist);

/** Resolve a served path the way a static host would, or return null. */
const resolve = (urlPath) => {
	const clean = urlPath.split(/[#?]/)[0];
	if (!clean.startsWith(`${base}/`) && clean !== base) return null;
	const rest = clean.slice(base.length) || '/';
	for (const candidate of [join(dist, rest), join(dist, rest, 'index.html'), `${join(dist, rest)}.html`]) {
		if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
	}
	return null;
};

// 1. Every root-relative link and asset reference in the rendered HTML.
for (const file of files.filter((f) => f.endsWith('.html'))) {
	const html = readFileSync(file, 'utf8');
	for (const [, url] of html.matchAll(/(?:href|src)="(\/[^"]*)"/g)) {
		if (resolve(url) === null) fail(`${relative(dist, file)} → ${url}`);
	}
}

// 2. Every page in the search index, resolved the way Pagefind resolves it at
// runtime: it derives its own base by stripping `pagefind/` off the path the bundle
// was loaded from, then prefixes that onto each stored URL. The UI must pass those
// URLs through untouched — prefixing the base a second time is the bug.
const fragments = join(dist, 'pagefind', 'fragment');
if (!existsSync(fragments)) {
	fail('dist/pagefind/fragment is missing — the search index did not build');
} else {
	const indexed = readdirSync(fragments).filter((f) => f.endsWith('.pf_fragment'));
	if (indexed.length === 0) fail('the search index contains no pages');
	for (const name of indexed) {
		const raw = decompress(readFileSync(join(fragments, name))).toString('utf8');
		const { url } = JSON.parse(raw.slice(raw.indexOf('{')));
		const served = `${base}/${url}`.replace(/\/+/g, '/');
		if (resolve(served) === null) fail(`search result → ${served} (indexed as ${url})`);
	}
}

// 3. A link into the repo that names a branch breaks the day that branch is renamed.
// `blob/HEAD` follows the default branch instead.
for (const file of files.filter((f) => f.endsWith('.html'))) {
	const html = readFileSync(file, 'utf8');
	for (const [match] of html.matchAll(/https:\/\/github\.com\/[^"]*\/(?:blob|tree|raw)\/(?!HEAD)[^/"]+\//g)) {
		fail(`${relative(dist, file)} pins a branch name: ${match} — use blob/HEAD instead`);
	}
}

if (failures.length > 0) {
	console.error(`Broken links (${failures.length}):`);
	for (const line of failures) console.error(`  ${line}`);
	process.exit(1);
}

console.log(`Links OK: ${files.filter((f) => f.endsWith('.html')).length} pages, base ${base || '/'}`);
