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
		// The /desktop hero renders the desktop app's own components from source rather
		// than copies of them, so the dev server has to be allowed to read outside this
		// project, and React has to resolve to this project's single copy — apps/desktop
		// keeps its own node_modules, and two Reacts means the hooks throw.
		resolve: { dedupe: ['react', 'react-dom'] },
		server: { fs: { allow: ['..'] } },
	},
});
