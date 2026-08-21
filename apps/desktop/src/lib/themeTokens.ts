// Handing CSS colours to something that cannot read CSS.
//
// ECharts draws to a canvas and takes literal colours, so the one place the brand
// tokens live has to be read out and passed in. `getComputedStyle` returns the
// substituted value, which is what makes this work: `--surface: var(--paper)` reads
// back as `#fbf8ff`, not as the reference.

export interface Tokens {
	surface: string;
	surface2: string;
	text: string;
	textMuted: string;
	textSubtle: string;
	border: string;
	borderSubtle: string;
	focus: string;
	fail: string;
	dark: boolean;
}

const NAMES = {
	surface: '--surface',
	surface2: '--surface-2',
	text: '--text',
	textMuted: '--text-muted',
	textSubtle: '--text-subtle',
	border: '--border',
	borderSubtle: '--border-subtle',
	focus: '--focus',
	fail: '--fail',
} as const;

/** Values a canvas can use if the document is not available (tests, SSR). */
export const FALLBACK: Tokens = {
	surface: '#fbf8ff',
	surface2: '#e7e6e9',
	text: '#020d0c',
	textMuted: '#6e7776',
	textSubtle: '#3a4443',
	border: '#b6bcbb',
	borderSubtle: '#e7e6e9',
	focus: '#ca2e55',
	fail: '#b45309',
	dark: false,
};

export const readTokens = (root?: HTMLElement): Tokens => {
	const element = root ?? (typeof document === 'undefined' ? null : document.documentElement);
	if (!element || typeof getComputedStyle !== 'function') return FALLBACK;

	const style = getComputedStyle(element);
	const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;

	return {
		surface: read(NAMES.surface, FALLBACK.surface),
		surface2: read(NAMES.surface2, FALLBACK.surface2),
		text: read(NAMES.text, FALLBACK.text),
		textMuted: read(NAMES.textMuted, FALLBACK.textMuted),
		textSubtle: read(NAMES.textSubtle, FALLBACK.textSubtle),
		border: read(NAMES.border, FALLBACK.border),
		borderSubtle: read(NAMES.borderSubtle, FALLBACK.borderSubtle),
		focus: read(NAMES.focus, FALLBACK.focus),
		fail: read(NAMES.fail, FALLBACK.fail),
		dark: element.getAttribute('data-bs-theme') === 'dark',
	};
};

/**
 * Watch for the theme changing.
 *
 * The attribute is the only thing that moves the tokens, so one observer on it is
 * sufficient and costs nothing for the life of the window.
 */
export const observeTokens = (onChange: (tokens: Tokens) => void): (() => void) => {
	if (typeof MutationObserver !== 'function' || typeof document === 'undefined') {
		return () => {};
	}
	const observer = new MutationObserver(() => onChange(readTokens()));
	observer.observe(document.documentElement, {
		attributes: true,
		attributeFilter: ['data-bs-theme'],
	});
	return () => observer.disconnect();
};
