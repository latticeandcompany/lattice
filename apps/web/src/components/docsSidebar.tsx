import { useState } from 'react';
import type { DocGroup } from '../lib/docs';

interface DocsSidebarProps {
	nav: DocGroup[];
	path: string;
}

const isActive = (href: string, path: string) => {
	const norm = (p: string) => (p !== '/' && p.endsWith('/') ? p.slice(0, -1) : p);
	return norm(href) === norm(path);
};

// Bootstrap nav (vertical), with a collapse for the mobile disclosure.
const DocsSidebar = ({ nav, path }: DocsSidebarProps) => {
	const [open, setOpen] = useState(false);

	return (
		<nav aria-label="Documentation">
			<button
				type="button"
				className="btn btn-outline-secondary btn-sm d-lg-none w-100 d-flex align-items-center justify-content-between mb-3"
				aria-expanded={open}
				onClick={() => setOpen((v) => !v)}
			>
				<span>Documentation menu</span>
				<i className={`bi ${open ? 'bi-chevron-up' : 'bi-chevron-down'}`} aria-hidden="true" />
			</button>

			<div className={`collapse d-lg-block${open ? ' show' : ''}`}>
				{nav.map((group) => (
					<div key={group.title} className="mb-4">
						<div
							className="text-uppercase fw-medium mb-2 px-2"
							style={{ fontSize: '0.72rem', letterSpacing: '0.08em', color: 'var(--text-muted)' }}
						>
							{group.title}
						</div>
						<ul className="nav flex-column">
							{group.links.map((link) => {
								const active = isActive(link.href, path);
								return (
									<li className="nav-item" key={link.href}>
										<a
											href={link.href}
											className={`nav-link py-1 px-2 doc-nav-link${active ? ' active' : ''}`}
											aria-current={active ? 'page' : undefined}
										>
											{link.title}
										</a>
									</li>
								);
							})}
						</ul>
					</div>
				))}
			</div>
		</nav>
	);
};

export default DocsSidebar;
