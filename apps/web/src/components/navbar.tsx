import { useEffect, useState } from 'react';
import { withBase } from '../lib/base';
import { getStartedUrl, navLinks } from '../lib/nav';
import Rosette from './rosette';
import ThemeControl from './themeControl';

// Bootstrap navbar. Collapse is driven by React state rather than Bootstrap's JS so
// the island stays self-contained. The mark draws itself on load.
const Navbar = () => {
	const [open, setOpen] = useState(false);
	const [scrolled, setScrolled] = useState(false);

	useEffect(() => {
		const onScroll = () => setScrolled(window.scrollY > 8);
		onScroll();
		window.addEventListener('scroll', onScroll, { passive: true });
		return () => window.removeEventListener('scroll', onScroll);
	}, []);

	return (
		<nav
			className="navbar navbar-expand-md sticky-top py-2"
			style={{
				background: scrolled ? 'color-mix(in srgb, var(--surface) 82%, transparent)' : 'var(--surface)',
				backdropFilter: scrolled ? 'saturate(1.4) blur(10px)' : 'none',
				borderBottom: `1px solid ${scrolled ? 'var(--border-subtle)' : 'transparent'}`,
				transition: 'border-color 0.2s ease, background 0.2s ease',
			}}
		>
			<div className="container" style={{ maxWidth: '72rem' }}>
				<a className="navbar-brand d-inline-flex align-items-center gap-2" href={withBase('/')} style={{ color: 'var(--text)' }}>
					<Rosette size="1.75rem" />
					<span className="fw-medium" style={{ letterSpacing: 'var(--wordmark-tracking)' }}>lattice</span>
				</a>

				<button
					type="button"
					className="navbar-toggler border-0 p-2"
					aria-label="Toggle navigation"
					aria-expanded={open}
					onClick={() => setOpen((v) => !v)}
					style={{ color: 'var(--text)', boxShadow: 'none' }}
				>
					<i className={`bi ${open ? 'bi-x-lg' : 'bi-list'} fs-4`} aria-hidden="true" />
				</button>

				<div className={`collapse navbar-collapse${open ? ' show' : ''}`}>
					<ul className="navbar-nav ms-auto align-items-md-center gap-md-1">
						{navLinks.map((link) => (
							<li className="nav-item" key={link.name}>
								<a
									className="nav-link nav-link--quiet"
									href={link.href}
									{...(link.external ? { target: '_blank', rel: 'noopener noreferrer' } : {})}
									onClick={() => setOpen(false)}
								>
									{link.name}
								</a>
							</li>
						))}
						<li className="nav-item d-none d-md-block">
							<ThemeControl />
						</li>
						<li className="nav-item mt-2 mt-md-0 ms-md-1">
							<a href={getStartedUrl} className="btn btn-contrast btn-sm px-3 py-2" onClick={() => setOpen(false)}>
								Get started
							</a>
						</li>
						<li className="nav-item d-md-none mt-2">
							<ThemeControl />
						</li>
					</ul>
				</div>
			</div>
		</nav>
	);
};

export default Navbar;
