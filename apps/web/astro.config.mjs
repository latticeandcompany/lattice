import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';
import react from '@astrojs/react';
import sitemap from '@astrojs/sitemap';
import mdx from '@astrojs/mdx';

// https://astro.build/config
export default defineConfig({
	// GitHub Pages project site: served from the org domain under a /lattice subpath.
	// `base` must stay in sync with the $base in src/styles/_paths.scss, which CSS
	// needs because stylesheets can't read import.meta.env.BASE_URL.
	site: 'https://latticeandcompany.github.io',
	base: '/lattice',
	integrations: [react(), sitemap(), mdx()],
	vite: {
		plugins: [tailwindcss()],
	},
});
