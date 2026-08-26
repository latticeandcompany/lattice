// Two builds, because the two entry points want different things.
//
// The library is emitted twice, ESM and CJS, so `import` and `require` both reach
// binaryPath(). The bin is emitted once: it is run by Node as a program, never
// imported, so a second format would be a file nothing loads. It inlines the
// library rather than importing it, which keeps the bin a single file that a reader
// can follow top to bottom.
//
// The version is stamped in at build time rather than read from package.json at
// runtime. A published dist/ is then true about what it wraps no matter where it is
// unpacked, and nothing has to resolve its own package.json from inside a bundle.

import { readFileSync } from 'node:fs';

import { defineConfig } from 'rolldown';

const pkg = JSON.parse(
	readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
) as { version: string };

// `define` moved under `transform` in rolldown 1.2; a stray top-level one is only a warning.
const transform = { define: { __LATTICE_VERSION__: JSON.stringify(pkg.version) } };

export default defineConfig([
	{
		input: 'src/index.ts',
		platform: 'node',
		transform,
		output: [
			{ dir: 'dist', format: 'esm', entryFileNames: 'index.mjs', cleanDir: true },
			{ dir: 'dist', format: 'cjs', entryFileNames: 'index.cjs' },
		],
	},
	{
		input: 'src/cli.ts',
		platform: 'node',
		transform,
		output: {
			dir: 'dist',
			format: 'esm',
			entryFileNames: 'cli.mjs',
			banner: '#!/usr/bin/env node',
		},
	},
]);
