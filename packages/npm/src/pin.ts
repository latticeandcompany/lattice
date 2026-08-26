// Telling the truth about which version is about to run.
//
// A repo's lattice.json pins a latticeVersion, and a binary installed under
// .lattice/bin honors that pin by installing the named version and handing the
// invocation over to it. A binary installed from npm cannot do that and should not
// try: the lockfile already decided which version is here, and reaching out to
// GitHub mid-command to fetch a different one is the install path this package
// exists to replace. So the pin is read, the mismatch is reported, and the version
// npm installed runs.
//
// The suppression rules are the CLI's own, so one repo-wide setting quiets both:
// LATTICE_NO_VERSION_CHECK, `settings.versionCheck: false`, or --no-version-check
// on the command line.
//
// Parsing is deliberately lenient and does not validate. A lattice.json written
// against a newer schema than this package understands must still be able to say
// which version can read it.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

export interface Pin {
	version: string | null;
	versionCheck: boolean;
}

export const parsePin = (text: string): Pin => {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch {
		return { version: null, versionCheck: true };
	}
	if (typeof value !== 'object' || value === null) return { version: null, versionCheck: true };

	const config = value as { latticeVersion?: unknown; settings?: { versionCheck?: unknown } };
	return {
		version: typeof config.latticeVersion === 'string' ? config.latticeVersion : null,
		versionCheck: config.settings?.versionCheck === false ? false : true,
	};
};

/** The nearest lattice.json at or above `start`. */
export const findConfig = (start: string): string | null => {
	let dir = start;
	for (;;) {
		const candidate = join(dir, 'lattice.json');
		try {
			readFileSync(candidate);
			return candidate;
		} catch {
			const parent = dirname(dir);
			if (parent === dir) return null;
			dir = parent;
		}
	}
};

/**
 * The warning to print, or null when there is nothing to say.
 *
 * Kept separate from printing it so the decision can be tested without a console.
 */
export const driftWarning = (pinned: string | null, installed: string): string | null => {
	if (pinned === null || pinned === installed) return null;
	return (
		`lattice.json pins latticeVersion ${pinned}, but the installed package is ${installed}.\n` +
		`Running ${installed}. Reconcile them by updating one to match the other, or set\n` +
		`"settings": { "versionCheck": false } in lattice.json to silence this.`
	);
};

const suppressed = (argv: string[], env: NodeJS.ProcessEnv): boolean =>
	env.LATTICE_NO_VERSION_CHECK !== undefined || argv.includes('--no-version-check');

/** Print the drift warning to stderr, if there is one and nothing has silenced it. */
export const warnOnDrift = (
	installed: string,
	cwd: string = process.cwd(),
	argv: string[] = process.argv.slice(2),
	env: NodeJS.ProcessEnv = process.env,
): void => {
	if (suppressed(argv, env)) return;

	const config = findConfig(cwd);
	if (config === null) return;

	let pin: Pin;
	try {
		pin = parsePin(readFileSync(config, 'utf8'));
	} catch {
		return;
	}
	if (!pin.versionCheck) return;

	const warning = driftWarning(pin.version, installed);
	if (warning !== null) process.stderr.write(`lattice: ${warning}\n`);
};
