import { useCallback, useEffect, useRef, useState } from 'react';
import { withBase } from '../lib/base';

// Pagefind builds a static WASM-backed index into dist/pagefind at build time, so the
// bundle only exists in a built site. It is fetched at runtime (never bundled), hence
// the @vite-ignore: Vite must not try to resolve this path during the Astro build.
const bundlePath = withBase('/pagefind/pagefind.js');
const maxPages = 6;
const maxSections = 3;

interface PagefindSubResult {
	title: string;
	url: string;
	excerpt: string;
}

interface PagefindData {
	url: string;
	excerpt: string;
	meta: { title?: string };
	sub_results: PagefindSubResult[];
}

interface PagefindResult {
	id: string;
	data: () => Promise<PagefindData>;
}

interface PagefindApi {
	init: () => void;
	debouncedSearch: (
		query: string,
		options?: unknown,
		delay?: number,
	) => Promise<{ results: PagefindResult[] } | null>;
}

export interface SearchHit {
	title: string;
	href: string;
	excerpt: string;
	section?: string;
}

type Status = 'idle' | 'searching' | 'ready' | 'unavailable';

// One hit per page, plus its strongest heading matches, so a long page can point at
// the section that actually matched instead of just its title.
const toHits = (pages: PagefindData[]): SearchHit[] =>
	pages.flatMap((page) => {
		const title = page.meta.title ?? page.url;
		// Pagefind indexes dist/ as the root, so its URLs come back without the site base.
		const pageHref = withBase(page.url);
		const head: SearchHit = { title, href: pageHref, excerpt: page.excerpt };
		// Pagefind emits a sub-result for the h1 too, which just restates the page hit.
		const sections = page.sub_results
			.filter((sub) => sub.url !== page.url && sub.title !== title)
			.slice(0, maxSections)
			.map((sub) => ({ title, href: withBase(sub.url), excerpt: sub.excerpt, section: sub.title }));
		return [head, ...sections];
	});

const isTypingTarget = (target: EventTarget | null) => {
	if (!(target instanceof HTMLElement)) return false;
	return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
};

const DocsSearch = () => {
	const [open, setOpen] = useState(false);
	const [query, setQuery] = useState('');
	const [hits, setHits] = useState<SearchHit[]>([]);
	const [status, setStatus] = useState<Status>('idle');
	const [active, setActive] = useState(0);
	const inputRef = useRef<HTMLInputElement>(null);
	const pagefind = useRef<PagefindApi | null>(null);
	const opener = useRef<Element | null>(null);

	const load = useCallback(async () => {
		if (pagefind.current) return pagefind.current;
		try {
			const module = (await import(/* @vite-ignore */ bundlePath)) as PagefindApi;
			module.init();
			pagefind.current = module;
			return module;
		} catch {
			setStatus('unavailable');
			return null;
		}
	}, []);

	const show = useCallback(() => {
		opener.current = document.activeElement;
		setOpen(true);
		void load();
	}, [load]);

	const hide = useCallback(() => {
		setOpen(false);
		setQuery('');
		setHits([]);
		setActive(0);
		if (opener.current instanceof HTMLElement) opener.current.focus();
	}, []);

	// ⌘K / Ctrl-K anywhere, and `/` when the reader isn't already typing.
	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			const shortcut = event.key.toLowerCase() === 'k' && (event.metaKey || event.ctrlKey);
			const slash = event.key === '/' && !isTypingTarget(event.target);
			if (!shortcut && !slash) return;
			event.preventDefault();
			show();
		};
		document.addEventListener('keydown', onKeyDown);
		return () => document.removeEventListener('keydown', onKeyDown);
	}, [show]);

	useEffect(() => {
		if (!open) return;
		inputRef.current?.focus();
		const { overflow } = document.body.style;
		document.body.style.overflow = 'hidden';
		return () => {
			document.body.style.overflow = overflow;
		};
	}, [open]);

	useEffect(() => {
		if (!open) return;
		const term = query.trim();
		if (term.length === 0) {
			setHits([]);
			setStatus('idle');
			return;
		}

		let stale = false;
		setStatus('searching');
		void (async () => {
			const module = await load();
			if (!module || stale) return;
			const search = await module.debouncedSearch(term);
			// Pagefind returns null when a newer query superseded this one.
			if (search === null || stale) return;
			const pages = await Promise.all(search.results.slice(0, maxPages).map((result) => result.data()));
			if (stale) return;
			setHits(toHits(pages));
			setActive(0);
			setStatus('ready');
		})();

		return () => {
			stale = true;
		};
	}, [query, open, load]);

	const onFieldKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
		if (event.key === 'Escape') {
			event.preventDefault();
			hide();
			return;
		}
		if (hits.length === 0) return;
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			setActive((i) => (i + 1) % hits.length);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			setActive((i) => (i - 1 + hits.length) % hits.length);
		} else if (event.key === 'Enter') {
			event.preventDefault();
			window.location.href = hits[active].href;
		}
	};

	return (
		<>
			<button
				type="button"
				className="btn btn-outline-secondary btn-sm w-100 d-flex align-items-center gap-2 mb-4 doc-search-trigger"
				onClick={show}
			>
				<i className="bi bi-search" aria-hidden="true" />
				<span className="tw:flex-1 tw:text-left">Search docs</span>
				<kbd className="doc-search-kbd">⌘K</kbd>
			</button>

			{open && (
				<div className="doc-search-backdrop" onMouseDown={hide}>
					<div
						className="doc-search-panel"
						role="dialog"
						aria-modal="true"
						aria-label="Search documentation"
						onMouseDown={(event) => event.stopPropagation()}
					>
						<div className="d-flex align-items-center gap-2 px-3 doc-search-field">
							<i className="bi bi-search" aria-hidden="true" style={{ color: 'var(--text-muted)' }} />
							<input
								ref={inputRef}
								type="search"
								className="form-control border-0 shadow-none px-0"
								placeholder="Search documentation"
								aria-label="Search documentation"
								autoComplete="off"
								value={query}
								onChange={(event) => setQuery(event.target.value)}
								onKeyDown={onFieldKeyDown}
								style={{ background: 'transparent' }}
							/>
							<button type="button" className="btn btn-sm btn-link text-decoration-none" onClick={hide}>
								<kbd className="doc-search-kbd">Esc</kbd>
							</button>
						</div>

						<div className="doc-search-results">
							{status === 'unavailable' && (
								<p className="doc-search-note">
									The search index is missing. It is generated at build time — run{' '}
									<code>npm run build</code>, then <code>npm run preview</code>.
								</p>
							)}
							{status === 'searching' && hits.length === 0 && <p className="doc-search-note">Searching…</p>}
							{status === 'ready' && hits.length === 0 && (
								<p className="doc-search-note">No results for “{query.trim()}”.</p>
							)}
							{status === 'idle' && (
								<p className="doc-search-note">Type to search. ↑↓ to move, ↵ to open, Esc to close.</p>
							)}

							{hits.length > 0 && (
								<ul className="list-unstyled mb-0">
									{hits.map((hit, index) => (
										<li key={`${hit.href}-${index}`}>
											<a
												href={hit.href}
												className={`doc-search-hit${index === active ? ' active' : ''}`}
												onMouseEnter={() => setActive(index)}
											>
												<span className="doc-search-hit__title">
													{hit.section ? `${hit.title} › ${hit.section}` : hit.title}
												</span>
												{/* Pagefind escapes text and returns <mark> around matched terms. */}
												<span
													className="doc-search-hit__excerpt"
													dangerouslySetInnerHTML={{ __html: hit.excerpt }}
												/>
											</a>
										</li>
									))}
								</ul>
							)}
						</div>
					</div>
				</div>
			)}
		</>
	);
};

export default DocsSearch;
