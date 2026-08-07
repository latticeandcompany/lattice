// The actual image URLs.
//
// Separate from `languages.ts` because a bundler resolves these imports and the test
// runner does not, and keeping them apart is what lets the mark logic be tested.
//
// The images are committed at the size they are displayed. The site's copies run to
// 2MB each because Astro resizes them at build time; Vite has no equivalent, so a
// pre-sized copy is the honest way to keep the bundle small.

import dotnet from '../assets/languages/dotnet.png';
import go from '../assets/languages/go.png';
import java from '../assets/languages/java.png';
import node from '../assets/languages/node.png';
import nodeDark from '../assets/languages/node-dark.png';
import python from '../assets/languages/python.png';
import ruby from '../assets/languages/ruby.png';

export interface Art {
	light: string;
	/** Only where a logo would otherwise disappear against ink. */
	dark?: string;
}

export const ART: Record<string, Art> = {
	node: { light: node, dark: nodeDark },
	go: { light: go },
	java: { light: java },
	ruby: { light: ruby },
	python: { light: python },
	dotnet: { light: dotnet },
};

/** Which image to use for the theme in play. */
export const artFor = (slug: string, dark: boolean): string | undefined => {
	const art = ART[slug];
	if (!art) return undefined;
	return dark && art.dark ? art.dark : art.light;
};
