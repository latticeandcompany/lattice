// The actual image URLs.
//
// Separate from `languages.ts` because a bundler resolves these imports and the test
// runner does not, and keeping them apart is what lets the mark logic be tested.
//
// Vector, not raster: a mark is drawn at 1.75rem and again on a HiDPI screen, and the
// whole set costs less than one of the site's PNGs. It also means adding an ecosystem
// is one file with nothing to pre-size.
//
// The artwork is each ecosystem's own logo, from devicon (MIT). `dotnet.svg` is the
// one composed here, because devicon still ships the retired ".NET Core" lettering —
// the wordmark is Simple Icons' (CC0) on the official purple.

import dart from '../assets/languages/dart.svg';
import dotnet from '../assets/languages/dotnet.svg';
import elixir from '../assets/languages/elixir.svg';
import go from '../assets/languages/go.svg';
import haskell from '../assets/languages/haskell.svg';
import java from '../assets/languages/java.svg';
import kotlin from '../assets/languages/kotlin.svg';
import node from '../assets/languages/node.svg';
import php from '../assets/languages/php.svg';
import python from '../assets/languages/python.svg';
import ruby from '../assets/languages/ruby.svg';
import rust from '../assets/languages/rust.svg';
import swift from '../assets/languages/swift.svg';

export const ART: Record<string, string> = {
	node,
	rust,
	go,
	python,
	ruby,
	java,
	kotlin,
	dotnet,
	swift,
	php,
	elixir,
	dart,
	haskell,
};

export const artFor = (slug: string): string | undefined => ART[slug];
