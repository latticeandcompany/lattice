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
		// project, and their bare imports have to resolve to this project's copies.
		//
		// Every name here is one those files import. A bare specifier resolves from the
		// importing file's directory, so `echarts/core` inside apps/desktop/src/hooks
		// finds apps/desktop/node_modules — which exists on a dev machine and never in
		// CI, where only this workspace is installed. React needs it for a second
		// reason: two copies means the hooks throw.
		resolve: { dedupe: ['react', 'react-dom', 'echarts', 'bootstrap-icons'] },
		server: { fs: { allow: ['..'] } },
	},
});
