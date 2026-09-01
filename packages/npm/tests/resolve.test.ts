// The built package, exercised the way npm will run it.
//
// Everything else here tests source modules directly, which proves the logic and
// nothing about the bundle. These tests run dist/ instead: they stand up a fake
// platform package in a scratch node_modules and then require the CJS build, import
// the ESM build, and run the bin as a program. That covers the three things that can
// only break after bundling — dual-format resolution, argument forwarding, and exit
// codes — and it is why `test` depends on `build`.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
	chmodSync,
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	realpathSync,
	rmSync,
	writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, before, describe, test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { resolveTarget, SCOPE } from '../src/target.ts';

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, '..', 'dist');

// Read rather than written out: the bundler substitutes __LATTICE_VERSION__ from
// this same file, so a literal here would only assert that someone remembered to
// edit two places at once.
const { version: packageVersion } = createRequire(import.meta.url)('../package.json') as {
	version: string;
};

const target = resolveTarget();
const windows = process.platform === 'win32';

let scratch = '';
let fakeExe = '';

describe('the built package', { skip: target === null ? 'no binary is published for this platform' : false }, () => {
	before(() => {
		assert.ok(target !== null);
		assert.ok(existsSync(join(dist, 'cli.mjs')), 'dist is missing — run `npm run build` first');

		// realpath, because require.resolve reports one and macOS points /var at
		// /private/var — otherwise every path assertion here fails on a symlink.
		scratch = realpathSync(mkdtempSync(join(tmpdir(), 'lattice-npm-dist-')));
		const pkg = join(scratch, 'pkg');
		mkdirSync(pkg, { recursive: true });
		cpSync(dist, pkg, { recursive: true });
		writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: 'host', type: 'module' }));

		// The fake platform package: a real directory, resolved by the real resolver.
		const platform = join(pkg, 'node_modules', SCOPE, target.pkg);
		mkdirSync(join(platform, 'bin'), { recursive: true });
		writeFileSync(
			join(platform, 'package.json'),
			JSON.stringify({ name: `${SCOPE}/${target.pkg}`, version: '0.0.0-test' }),
		);

		fakeExe = join(platform, 'bin', target.exe);
		// Stands in for the binary: echoes what it was handed, exits with a code the
		// test can recognise as its own rather than as an accident.
		writeFileSync(
			fakeExe,
			windows
				? '@echo off\r\necho ARGS:%*\r\nexit /b 7\r\n'
				: '#!/bin/sh\nprintf "ARGS:%s\\n" "$*"\nexit 7\n',
		);
		if (!windows) chmodSync(fakeExe, 0o755);
	});

	after(() => rmSync(scratch, { recursive: true, force: true }));

	test('require() finds the binary in the sibling platform package', () => {
		const req = createRequire(join(scratch, 'pkg', 'noop.cjs'));
		const api = req('./index.cjs') as { binaryPath: () => string; version: string };
		assert.equal(api.binaryPath(), fakeExe);
		assert.equal(api.version, packageVersion);
	});

	test('import finds the same binary', async () => {
		const url = pathToFileURL(join(scratch, 'pkg', 'index.mjs')).href;
		const api = (await import(url)) as { binaryPath: () => string; version: string };
		assert.equal(api.binaryPath(), fakeExe);
		assert.equal(api.version, packageVersion);
	});

	test('the bin forwards its arguments and returns the exit code', { skip: windows }, () => {
		const result = spawnSync(
			process.execPath,
			[join(scratch, 'pkg', 'cli.mjs'), 'run', 'build', '--filter=web'],
			{ encoding: 'utf8', cwd: scratch },
		);
		assert.equal(result.status, 7);
		assert.match(result.stdout, /ARGS:run build --filter=web/);
	});

	test('a missing platform package is an explanation, not a stack trace', () => {
		const bare = join(scratch, 'bare');
		mkdirSync(bare, { recursive: true });
		cpSync(dist, bare, { recursive: true });
		writeFileSync(join(bare, 'package.json'), JSON.stringify({ name: 'bare', type: 'module' }));

		const req = createRequire(join(bare, 'noop.cjs'));
		const api = req('./index.cjs') as { binaryPath: () => string };
		assert.throws(api.binaryPath, (error: Error) => {
			assert.match(error.message, /Lattice installed, but .* did not\./);
			assert.match(error.message, /optional/);
			assert.ok(target !== null);
			assert.ok(error.message.includes(target.triple), 'the error should name the release asset');
			return true;
		});
	});
});
