import { withBase } from './base';

export interface NavLink {
	name: string;
	href: string;
	external?: boolean;
}

export const navLinks: NavLink[] = [
	{ name: 'Docs', href: withBase('/docs') },
	{ name: 'Why Lattice', href: withBase('/#why') },
	{ name: 'GitHub', href: 'https://github.com/latticeandcompany/lattice', external: true },
];

export const githubUrl = 'https://github.com/latticeandcompany/lattice';
