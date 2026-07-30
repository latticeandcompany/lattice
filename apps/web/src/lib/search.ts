// Shaping Pagefind's results into the list the search dialog renders. Kept out of the
// component so it can be tested without a DOM.

export interface PagefindSubResult {
	title: string;
	url: string;
	excerpt: string;
}

export interface PagefindData {
	url: string;
	excerpt: string;
	meta: { title?: string };
	sub_results: PagefindSubResult[];
}

export interface SearchHit {
	title: string;
	href: string;
	excerpt: string;
	section?: string;
}

export const maxPages = 6;
export const maxSections = 3;

/**
 * One hit per page, plus its strongest heading matches, so a long page can point at
 * the section that actually matched instead of just its title.
 *
 * URLs are passed through untouched. Pagefind derives its own base by stripping
 * `pagefind/` off the path its bundle was loaded from, which already carries the site
 * base, so prefixing the base again here would produce `/lattice/lattice/...`.
 */
export const toHits = (pages: PagefindData[]): SearchHit[] =>
	pages.flatMap((page) => {
		const title = page.meta.title ?? page.url;
		const head: SearchHit = { title, href: page.url, excerpt: page.excerpt };
		// Pagefind emits a sub-result for the h1 too, which just restates the page hit.
		const sections = page.sub_results
			.filter((sub) => sub.url !== page.url && sub.title !== title)
			.slice(0, maxSections)
			.map((sub) => ({ title, href: sub.url, excerpt: sub.excerpt, section: sub.title }));
		return [head, ...sections];
	});
