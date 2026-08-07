// Light, dark, or whatever the OS says.
//
// The attribute on <html> is what actually paints the app: both the CSS custom
// properties and Tailwind's dark variant key off `data-bs-theme`. Everything else
// here exists to keep the native window frame in step with it.

import { setWindowTheme } from './api.ts';

export type Mode = 'light' | 'dark' | 'system';
export type Resolved = 'light' | 'dark';

export const MODES: { mode: Mode; label: string; icon: string }[] = [
	{ mode: 'light', label: 'Light', icon: 'bi-sun' },
	{ mode: 'dark', label: 'Dark', icon: 'bi-moon-stars' },
	{ mode: 'system', label: 'System', icon: 'bi-circle-half' },
];

export const prefersDark = (): boolean =>
	typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches;

export const resolve = (mode: Mode, systemPrefersDark: boolean): Resolved =>
	mode === 'system' ? (systemPrefersDark ? 'dark' : 'light') : mode;

export const storedMode = (): Mode => {
	try {
		const stored = localStorage.getItem('theme');
		return stored === 'light' || stored === 'dark' ? stored : 'system';
	} catch {
		return 'system';
	}
};

/**
 * Apply a mode everywhere it has to land.
 *
 * The window is told `null` for system rather than the resolved value, and that
 * ordering is load-bearing: handing it an explicit theme makes the *webview* report
 * that theme through `prefers-color-scheme`, so a later read of matchMedia would see
 * our own override instead of the operating system.
 */
export const applyMode = async (mode: Mode): Promise<Resolved> => {
	const resolved = resolve(mode, prefersDark());
	document.documentElement.setAttribute('data-bs-theme', resolved);
	try {
		localStorage.setItem('theme', mode);
	} catch {
		// Storage unavailable; the choice still holds for this session.
	}
	await setWindowTheme(mode === 'system' ? null : resolved);
	return resolved;
};

export const currentResolved = (): Resolved =>
	document.documentElement.getAttribute('data-bs-theme') === 'dark' ? 'dark' : 'light';
