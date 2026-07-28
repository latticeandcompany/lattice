import { useEffect, useRef, useState } from 'react';

type Mode = 'light' | 'dark' | 'system';

const OPTIONS: { mode: Mode; label: string; icon: string }[] = [
	{ mode: 'light', label: 'Light', icon: 'bi-sun' },
	{ mode: 'dark', label: 'Dark', icon: 'bi-moon-stars' },
	{ mode: 'system', label: 'System', icon: 'bi-circle-half' },
];

const prefersDark = () => window.matchMedia('(prefers-color-scheme: dark)').matches;

const applyMode = (mode: Mode) => {
	const resolved = mode === 'system' ? (prefersDark() ? 'dark' : 'light') : mode;
	document.documentElement.setAttribute('data-bs-theme', resolved);
	try {
		localStorage.setItem('theme', mode);
	} catch {
		// storage unavailable; the choice still applies for this session.
	}
};

// Light / Dark / System, as a Bootstrap dropdown. System follows the OS setting and
// updates live when it changes.
const ThemeControl = () => {
	const [mode, setMode] = useState<Mode>('system');
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		let stored: string | null = null;
		try {
			stored = localStorage.getItem('theme');
		} catch {}
		setMode(stored === 'light' || stored === 'dark' ? stored : 'system');
	}, []);

	// Track OS changes while in system mode.
	useEffect(() => {
		if (mode !== 'system') return;
		const mql = window.matchMedia('(prefers-color-scheme: dark)');
		const onChange = () => applyMode('system');
		mql.addEventListener('change', onChange);
		return () => mql.removeEventListener('change', onChange);
	}, [mode]);

	// Close on outside click.
	useEffect(() => {
		if (!open) return;
		const onDown = (e: MouseEvent) => {
			if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
		};
		document.addEventListener('mousedown', onDown);
		return () => document.removeEventListener('mousedown', onDown);
	}, [open]);

	const choose = (next: Mode) => {
		setMode(next);
		applyMode(next);
		setOpen(false);
	};

	const current = OPTIONS.find((o) => o.mode === mode) ?? OPTIONS[2];

	return (
		<div className="dropdown" ref={ref}>
			<button
				type="button"
				className="btn btn-sm border-0 d-inline-flex align-items-center justify-content-center p-2"
				onClick={() => setOpen((v) => !v)}
				aria-haspopup="menu"
				aria-expanded={open}
				aria-label={`Theme: ${current.label}`}
				title={`Theme: ${current.label}`}
				style={{ color: 'var(--text-muted)', lineHeight: 1 }}
			>
				<i className={`bi ${current.icon}`} aria-hidden="true" />
			</button>
			<ul className={`dropdown-menu dropdown-menu-end${open ? ' show' : ''}`} style={{ minWidth: '9rem' }}>
				{OPTIONS.map((o) => (
					<li key={o.mode}>
						<button type="button" className={`dropdown-item d-flex align-items-center gap-2${o.mode === mode ? ' active' : ''}`} onClick={() => choose(o.mode)}>
							<i className={`bi ${o.icon}`} aria-hidden="true" />
							<span>{o.label}</span>
						</button>
					</li>
				))}
			</ul>
		</div>
	);
};

export default ThemeControl;
