// Which mark stands for a driver.
//
// The driver-to-ecosystem mapping is not here: it lives on DriverSpec in Rust, so a
// new driver cannot be added without choosing one, and the backend catalog hands it
// over. What is here is the second half — what to draw for an ecosystem.
//
// Every ecosystem Lattice can detect has artwork. The monogram is what a driver with
// no ecosystem falls back to — one of the agnostic task runners — so a workspace still
// shows a deliberate mark rather than a missing image.
//
// No asset imports in this file. The URLs live in `languageArt.ts` because a bundler
// resolves an SVG import and the test runner does not, and this half is the half worth
// testing.

/** Every ecosystem a driver can carry. Kept in step with `languageArt.ts` by a test. */
export const ART_SLUGS = [
	'node',
	'rust',
	'go',
	'python',
	'ruby',
	'java',
	'kotlin',
	'dotnet',
	'swift',
	'php',
	'elixir',
	'dart',
	'haskell',
] as const;

export interface LanguageMark {
	kind: 'art' | 'monogram' | 'none';
	/** The ecosystem slug, when there is artwork to look up. */
	slug?: string;
	monogram?: string;
	/** What a reader hears, and what the tooltip says. */
	title: string;
}

const hasArt = (language: string): boolean => (ART_SLUGS as readonly string[]).includes(language);

/**
 * The mark for a driver.
 *
 * `language` comes from the backend catalog. A driver with none is one of the agnostic
 * task runners, which genuinely has no ecosystem to show.
 */
export const languageMark = (
	tool: string | null | undefined,
	language: string | null | undefined,
): LanguageMark => {
	if (!tool) {
		return { kind: 'none', title: 'No tool found to run this' };
	}
	if (!language) {
		// A task runner runs whatever the directory happens to be.
		return { kind: 'monogram', monogram: monogramFor(tool), title: tool };
	}
	if (hasArt(language)) {
		return { kind: 'art', slug: language, title: `${tool} (${language})` };
	}
	return { kind: 'monogram', monogram: monogramFor(language), title: `${tool} (${language})` };
};

/** Title case of the first two letters, so `just` never shows as `ju`. */
const monogramFor = (slug: string): string =>
	slug.slice(0, 2).replace(/^./, (character) => character.toUpperCase());
