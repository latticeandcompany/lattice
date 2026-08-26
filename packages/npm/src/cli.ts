// The `lattice` command, as npm installs it.
//
// Everything after the program name goes to the real binary untouched — no parsing,
// no rewriting, no flags of our own — so `npx lattice` and an installed `lattice`
// mean the same thing and the CLI's own help is the only help there is.
//
// spawnSync rather than spawn, because blocking is the correct behavior here.
// Ctrl-C reaches the whole process group, and while spawnSync holds the event loop
// Node cannot act on the signal first: the child handles it, tears down its own
// children, and exits, and only then does this process report what happened. A
// non-blocking spawn would have Node racing its own child to exit.

import { spawnSync } from 'node:child_process';
import { constants } from 'node:os';

import { binaryPath, version } from './index.ts';
import { warnOnDrift } from './pin.ts';

const signalExit = (signal: NodeJS.Signals): number => {
	const number = (constants.signals as Record<string, number>)[signal];
	return typeof number === 'number' ? 128 + number : 1;
};

const main = (): number => {
	let exe: string;
	try {
		exe = binaryPath();
	} catch (error) {
		process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
		return 1;
	}

	warnOnDrift(version);

	const result = spawnSync(exe, process.argv.slice(2), { stdio: 'inherit' });

	if (result.error !== undefined) {
		process.stderr.write(`lattice: could not run ${exe}: ${result.error.message}\n`);
		return 1;
	}
	if (result.signal !== null) return signalExit(result.signal);
	return result.status ?? 1;
};

// exitCode rather than process.exit(), which does not flush a pending write. The
// drift warning goes to stderr, and stderr is asynchronous when it is a pipe rather
// than a terminal — exiting outright can drop it on the way to a log file.
process.exitCode = main();
