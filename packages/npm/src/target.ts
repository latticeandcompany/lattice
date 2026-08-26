// The six targets release.yml builds, in npm's vocabulary and in Rust's.
//
// One table serves three readers: the runtime picks a row for the machine it is on,
// the publish script turns every row into a platform package, and the wrapper's
// optionalDependencies name every row. They cannot drift, because there is nothing
// to drift from.
//
// The two vocabularies do not line up — npm says `win32`/`x64`, Rust says
// `pc-windows-msvc`/`x86_64` — so both are written out. The npm spelling resolves
// the dependency; the triple is what a release asset is called, which is what lets
// an error name an archive the reader could go and fetch by hand.
//
// libc is the part npm cannot be trusted with. The `libc` field exists and is set
// below, but it is honored inconsistently across package managers and versions, so
// both Linux x64 packages install and the row is chosen here at runtime instead.

export const SCOPE = '@latticeandcompany';

export interface Target {
	/** The package that carries the binary, without the scope. */
	pkg: string;
	/** The Rust target triple, so an error can name the release asset. */
	triple: string;
	/** The binary's name inside that package. */
	exe: string;
	/** npm's `os`. */
	os: NodeJS.Platform;
	/** npm's `cpu`. */
	cpu: string;
	/** npm's `libc`, where the platform has more than one. */
	libc: 'glibc' | 'musl' | null;
}

const make = (
	pkg: string,
	triple: string,
	os: NodeJS.Platform,
	cpu: string,
	libc: 'glibc' | 'musl' | null,
): Target => ({ pkg, triple, exe: os === 'win32' ? 'lattice.exe' : 'lattice', os, cpu, libc });

/** Every platform package, in the order the wrapper lists them. */
export const ALL_TARGETS: Target[] = [
	make('lattice-darwin-arm64', 'aarch64-apple-darwin', 'darwin', 'arm64', null),
	make('lattice-darwin-x64', 'x86_64-apple-darwin', 'darwin', 'x64', null),
	make('lattice-linux-arm64-gnu', 'aarch64-unknown-linux-gnu', 'linux', 'arm64', 'glibc'),
	make('lattice-linux-x64-gnu', 'x86_64-unknown-linux-gnu', 'linux', 'x64', 'glibc'),
	make('lattice-linux-x64-musl', 'x86_64-unknown-linux-musl', 'linux', 'x64', 'musl'),
	make('lattice-win32-x64-msvc', 'x86_64-pc-windows-msvc', 'win32', 'x64', null),
];

/** Whether the running Linux is glibc rather than musl. */
const isGlibc = (): boolean => {
	const report = process.report?.getReport();
	if (typeof report !== 'object' || report === null) return true;
	const header = (report as { header?: { glibcVersionRuntime?: string } }).header;
	return typeof header?.glibcVersionRuntime === 'string';
};

/**
 * The row for this machine, or null when nothing is published for it.
 *
 * Every input is a parameter with a live default, so a test can ask about a platform
 * it is not running on. `glibc` in particular can only be discovered on the machine
 * itself, and a macOS test asking about Linux would otherwise be told musl.
 *
 * Windows on ARM is pointed at the x64 row on purpose: there is no
 * aarch64-pc-windows-msvc release yet and Windows runs x64 binaries under emulation.
 * macOS on ARM gets no such fallback, because a native build is published and
 * Rosetta is never the better answer. Linux arm64 musl simply has no row, so it
 * resolves to null and says so.
 */
export const resolveTarget = (
	platform: NodeJS.Platform = process.platform,
	arch: string = process.arch,
	glibc: boolean = isGlibc(),
): Target | null => {
	const cpu = platform === 'win32' && arch === 'arm64' ? 'x64' : arch;
	const libc = platform === 'linux' ? (glibc ? 'glibc' : 'musl') : null;
	return (
		ALL_TARGETS.find(
			(target) => target.os === platform && target.cpu === cpu && target.libc === libc,
		) ?? null
	);
};
