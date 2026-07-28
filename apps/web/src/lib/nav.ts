export interface NavLink {
	name: string;
	href: string;
	external?: boolean;
}

export const navLinks: NavLink[] = [
	{ name: 'Docs', href: '/docs' },
	{ name: 'Why Lattice', href: '/#why' },
	{ name: 'GitHub', href: 'https://github.com/lattice-and-co', external: true },
];

export const githubUrl = 'https://github.com/lattice-and-co';
