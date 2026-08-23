import { githubUrl } from './nav.ts';

/**
 * Every desktop download the site offers, and the one place their names are written.
 * This endpoint resolves an exact asset name, so a name cannot carry a version in it.
 * That is why release.yml stages each bundle twice: once under its version, and once
 * under the bare name below. The site then needs no redeploy after a release.
 *
 * GitHub's `latest` skips pre-releases, and release.yml marks any version with a `-`
 * in it as one. These links resolve once a stable version ships.
 */
export const latestDownloadBase = `${githubUrl}/releases/latest/download`;

export const downloadUrl = (asset: string) => `${latestDownloadBase}/${asset}`;

export type OsKey = 'macos' | 'windows' | 'linux';

export interface Download {
	/** What the file is, in the words a reader picking between them would use. */
	format: string;
	asset: string;
	/** Shown next to the format, so two rows on one platform are told apart. */
	note: string;
}

export interface Platform {
	os: OsKey;
	/** Tells two builds of one OS apart. Absent when the OS has a single build. */
	arch?: string;
	/** Heading form, for the downloads table. */
	label: string;
	/** No internal separator, so the picker can list these separated by one. */
	shortLabel: string;
	icon: string;
	downloads: Download[];
}

export const platforms: Platform[] = [
	{
		os: 'macos',
		arch: 'aarch64',
		label: 'macOS · Apple Silicon',
		shortLabel: 'Apple Silicon Mac',
		icon: 'bi-apple',
		downloads: [
			{
				format: 'Disk image',
				asset: 'lattice-desktop-macos-aarch64.dmg',
				note: 'Any Mac from 2020 on. Drag Lattice to Applications.',
			},
		],
	},
	{
		os: 'macos',
		arch: 'x86_64',
		label: 'macOS · Intel',
		shortLabel: 'Intel Mac',
		icon: 'bi-apple',
		downloads: [
			{
				format: 'Disk image',
				asset: 'lattice-desktop-macos-x86_64.dmg',
				note: 'Older Macs, before Apple Silicon.',
			},
		],
	},
	{
		os: 'windows',
		arch: 'x86_64',
		label: 'Windows · 64-bit',
		shortLabel: 'Windows',
		icon: 'bi-windows',
		downloads: [
			{
				format: 'Installer',
				asset: 'lattice-desktop-windows-x86_64-setup.exe',
				note: 'The one most people want. Installs for you, not the whole machine.',
			},
			{
				format: 'MSI package',
				asset: 'lattice-desktop-windows-x86_64.msi',
				note: 'For rolling it out with Group Policy or Intune.',
			},
		],
	},
	{
		os: 'linux',
		arch: 'x86_64',
		label: 'Linux · x86_64',
		shortLabel: 'Linux x86_64',
		icon: 'bi-tux',
		downloads: [
			{
				format: 'AppImage',
				asset: 'lattice-desktop-linux-x86_64.AppImage',
				note: 'Runs anywhere. Add the executable bit and go.',
			},
			{
				format: 'Debian package',
				asset: 'lattice-desktop-linux-x86_64.deb',
				note: 'Debian, Ubuntu, and anything built on them.',
			},
			{
				format: 'RPM package',
				asset: 'lattice-desktop-linux-x86_64.rpm',
				note: 'Fedora, RHEL, and openSUSE.',
			},
		],
	},
	{
		os: 'linux',
		arch: 'aarch64',
		label: 'Linux · aarch64',
		shortLabel: 'Linux aarch64',
		icon: 'bi-tux',
		downloads: [
			{
				format: 'AppImage',
				asset: 'lattice-desktop-linux-aarch64.AppImage',
				note: 'Runs anywhere. Add the executable bit and go.',
			},
			{
				format: 'Debian package',
				asset: 'lattice-desktop-linux-aarch64.deb',
				note: 'Debian, Ubuntu, and anything built on them, on ARM.',
			},
			{
				format: 'RPM package',
				asset: 'lattice-desktop-linux-aarch64.rpm',
				note: 'Fedora, RHEL, and openSUSE, on ARM.',
			},
		],
	},
];

/** The one download offered first for a platform, when the page has guessed one. */
export const primaryFor = (os: OsKey, arch: string) =>
	platforms.find((p) => p.os === os && p.arch === arch) ?? platforms[0];
