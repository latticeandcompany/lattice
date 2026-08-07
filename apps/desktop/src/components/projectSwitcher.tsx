import { useEffect, useRef, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { shortenPath } from '../lib/format.ts';

// Recent projects plus a way to open another. A React-controlled dropdown, matching
// how the theme control works rather than pulling in Bootstrap's JS for one menu.
const ProjectSwitcher = () => {
	const { recents, project, openProject, pickAndOpen, forget } = useApp();
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		if (!open) return;
		const onDown = (event: MouseEvent) => {
			if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false);
		};
		document.addEventListener('mousedown', onDown);
		return () => document.removeEventListener('mousedown', onDown);
	}, [open]);

	return (
		<div className="dropdown" ref={ref}>
			<button
				type="button"
				className="btn btn-sm border-0 d-inline-flex align-items-center gap-1 p-2"
				onClick={() => setOpen((value) => !value)}
				aria-haspopup="menu"
				aria-expanded={open}
				title="Switch project"
				style={{ color: 'var(--text-muted)', lineHeight: 1 }}
			>
				<i className="bi bi-arrow-left-right" aria-hidden="true" />
			</button>
			<ul
				className={`dropdown-menu${open ? ' show' : ''}`}
				style={{ minWidth: '16rem', bottom: '100%', top: 'auto' }}
			>
				{recents.length === 0 && (
					<li>
						<span className="dropdown-item-text text-body-secondary small">
							No projects opened yet.
						</span>
					</li>
				)}
				{recents.map((recent) => (
					<li key={recent.root} className="d-flex align-items-center">
						<button
							type="button"
							className={`dropdown-item${recent.root === project?.root ? ' active' : ''}`}
							onClick={() => {
								setOpen(false);
								void openProject(recent.root);
							}}
						>
							<span className="d-block">{recent.name}</span>
							<span
								className="d-block"
								style={{
									fontFamily: 'DM Mono, ui-monospace, monospace',
									fontSize: '0.7rem',
									color: 'var(--text-muted)',
								}}
							>
								{shortenPath(recent.root)}
							</span>
						</button>
						<button
							type="button"
							className="btn btn-sm border-0 px-2"
							title={`Forget ${recent.name}`}
							aria-label={`Forget ${recent.name}`}
							onClick={() => void forget(recent.root)}
							style={{ color: 'var(--text-muted)' }}
						>
							<i className="bi bi-x" aria-hidden="true" />
						</button>
					</li>
				))}
				<li>
					<hr className="dropdown-divider" />
				</li>
				<li>
					<button
						type="button"
						className="dropdown-item d-flex align-items-center gap-2"
						onClick={() => {
							setOpen(false);
							void pickAndOpen();
						}}
					>
						<i className="bi bi-folder2-open" aria-hidden="true" />
						<span>Open a folder…</span>
					</button>
				</li>
			</ul>
		</div>
	);
};

export default ProjectSwitcher;
