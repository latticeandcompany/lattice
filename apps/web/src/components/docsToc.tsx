import { useEffect, useState } from 'react';

export interface Heading {
	depth: number;
	slug: string;
	text: string;
}

interface DocsTocProps {
	headings: Heading[];
}

// On-this-page navigation (Bootstrap nav) with scroll-spy. Only h2/h3 are listed.
const DocsToc = ({ headings }: DocsTocProps) => {
	const items = headings.filter((h) => h.depth === 2 || h.depth === 3);
	const [active, setActive] = useState<string>('');

	useEffect(() => {
		if (items.length === 0) return;
		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) setActive(entry.target.id);
				}
			},
			{ rootMargin: '0px 0px -75% 0px', threshold: 1 },
		);
		items.forEach((h) => {
			const el = document.getElementById(h.slug);
			if (el) observer.observe(el);
		});
		return () => observer.disconnect();
	}, [items.length]);

	if (items.length === 0) return null;

	return (
		<nav aria-label="On this page">
			<div
				className="text-uppercase fw-medium mb-2 px-2"
				style={{ fontSize: '0.72rem', letterSpacing: '0.08em', color: 'var(--text-muted)' }}
			>
				On this page
			</div>
			<ul className="nav flex-column">
				{items.map((h) => {
					const current = active === h.slug;
					return (
						<li className="nav-item" key={h.slug} style={{ paddingLeft: h.depth === 3 ? '0.75rem' : 0 }}>
							<a href={`#${h.slug}`} className={`nav-link py-1 px-2 doc-toc-link${current ? ' active' : ''}`}>
								{h.text}
							</a>
						</li>
					);
				})}
			</ul>
		</nav>
	);
};

export default DocsToc;
