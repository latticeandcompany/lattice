import { useEffect, useRef, useState } from 'react';

import { MODES, applyMode, applyStoredMode, storedMode, type Mode } from '../lib/theme.ts';

// Light / Dark / System, as a Bootstrap dropdown driven by React rather than by
// Bootstrap's own JS. System follows the OS and updates live when it changes.
const ThemeControl = () => {
	const [mode, setMode] = useState<Mode>('system');
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		setMode(storedMode());
		void applyStoredMode();
	}, []);

	// While following the OS, track it changing.
	useEffect(() => {
		if (mode !== 'system') return;
		const query = window.matchMedia('(prefers-color-scheme: dark)');
		const onChange = () => void applyMode('system');
		query.addEventListener('change', onChange);
		return () => query.removeEventListener('change', onChange);
	}, [mode]);

	useEffect(() => {
		if (!open) return;
		const onDown = (event: MouseEvent) => {
			if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false);
		};
		document.addEventListener('mousedown', onDown);
		return () => document.removeEventListener('mousedown', onDown);
	}, [open]);

	const choose = (next: Mode) => {
		setMode(next);
		void applyMode(next);
		setOpen(false);
	};

	const current = MODES.find((option) => option.mode === mode) ?? MODES[2];

	return (
		<div className="dropdown" ref={ref}>
			<button
				type="button"
				className="btn btn-sm border-0 d-inline-flex align-items-center justify-content-center p-2 lh-1 text-body-secondary"
				onClick={() => setOpen((value) => !value)}
				aria-haspopup="menu"
				aria-expanded={open}
				aria-label={`Theme: ${current.label}`}
				title={`Theme: ${current.label}`}
			>
				<i className={`bi ${current.icon}`} aria-hidden="true" />
			</button>
			<ul
				className={`dropdown-menu${open ? ' show' : ''} tw:min-w-[9rem] tw:top-auto tw:bottom-full`}
			>
				{MODES.map((option) => (
					<li key={option.mode}>
						<button
							type="button"
							className={`dropdown-item d-flex align-items-center gap-2${option.mode === mode ? ' active' : ''}`}
							onClick={() => choose(option.mode)}
						>
							<i className={`bi ${option.icon}`} aria-hidden="true" />
							<span>{option.label}</span>
						</button>
					</li>
				))}
			</ul>
		</div>
	);
};

export default ThemeControl;
