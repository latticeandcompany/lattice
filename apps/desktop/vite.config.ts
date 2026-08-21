import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// Set by `tauri dev` when the dev server has to be reachable from a device
// rather than from localhost.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [tailwindcss(), react()],
	// The Tauri CLI owns this terminal; clearing it eats the Rust compiler's output.
	clearScreen: false,
	server: {
		// tauri.conf.json pins devUrl to this port, so falling back to another one
		// would leave the window pointed at nothing.
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
		// src-tauri churns on every cargo build; watching it restarts Vite in a loop.
		watch: { ignored: ['**/src-tauri/**'] },
	},
	envPrefix: ['VITE_', 'TAURI_ENV_*'],
	resolve: {
		alias: { '@': new URL('./src', import.meta.url).pathname },
	},
	build: {
		// The webview is fixed per platform, so target it rather than a browser
		// baseline. Tailwind v4 needs Safari 16.4 / Chrome 111 regardless, which is
		// the real floor here.
		target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome111' : 'safari16.4',
		minify: !process.env.TAURI_ENV_DEBUG,
		sourcemap: !!process.env.TAURI_ENV_DEBUG,
	},
});
