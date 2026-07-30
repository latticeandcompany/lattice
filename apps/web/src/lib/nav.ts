import { withBase } from './base';

export interface NavLink {
	name: string;
	href: string;
	external?: boolean;
}

export const navLinks: NavLink[] = [
	{ name: 'Docs', href: withBase('/docs') },
	{ name: 'For agents', href: withBase('/for-agents') },
	{ name: 'Why Lattice', href: withBase('/#why') },
	{ name: 'GitHub', href: 'https://github.com/latticeandcompany/lattice', external: true },
];

export const githubUrl = 'https://github.com/latticeandcompany/lattice';

/** Every "Get started" call to action on the site lands here. */
export const getStartedUrl = withBase('/get-started');

/**
 * `HEAD` resolves to whatever the repository's default branch is, so these keep
 * working if it's ever renamed. A hardcoded branch name does not.
 */
export const licenseUrl = `${githubUrl}/blob/HEAD/LICENSE`;
export const releasesUrl = `${githubUrl}/releases`;

/** Shown in the hero and the install card, so it lives in one place. */
export const installCommand = 'curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh';

/** Installs the agent skill in `skills/lattice/` into whichever agents the CLI finds. */
export const skillInstallCommand = 'npx skills add latticeandcompany/lattice';

/** Loads the same skill for one session without writing anything to disk. */
export const skillUseCommand = 'npx skills use latticeandcompany/lattice@lattice';
