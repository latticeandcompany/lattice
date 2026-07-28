/** @type {import('tailwindcss').Config} */
module.exports = {
	content: ['./src/**/*.{astro,html,js,jsx,md,mdx,ts,tsx}'],
	darkMode: ['selector', '[data-bs-theme="dark"]'],
	theme: {
		// Brand is monochrome-first (see marketing/BRAND.md). Base is ink/paper,
		// grays are the derived ramp, teal is the reserved product accent used
		// only for focus states — never as body color on standalone Lattice.
		colors: {
			transparent: 'transparent',
			current: 'currentColor',
			ink: '#020D0C',
			paper: '#FBF8FF',
			gray: {
				900: '#141F1E',
				700: '#3A4443',
				500: '#6E7776',
				300: '#B6BCBB',
				100: '#E7E6E9',
			},
			teal: {
				300: '#63C4B8',
				500: '#1B998B',
				700: '#136E64',
			},
		},
		fontFamily: {
			sans: ['DM Sans', 'system-ui', 'sans-serif'],
			mono: ['DM Mono', 'ui-monospace', 'monospace'],
		},
		extend: {
			fontSize: {
				// Brand type ramp (~1.25 modular scale)
				display: ['3.75rem', { lineHeight: '4rem', letterSpacing: '-0.02em', fontWeight: '700' }],
				h1: ['2.5rem', { lineHeight: '3rem', letterSpacing: '-0.01em', fontWeight: '700' }],
				h2: ['2rem', { lineHeight: '2.5rem', fontWeight: '700' }],
				h3: ['1.5rem', { lineHeight: '2rem', fontWeight: '500' }],
				'body-lg': ['1.125rem', { lineHeight: '1.75rem' }],
			},
			maxWidth: {
				content: '72rem',
			},
		},
	},
	plugins: [],
};
