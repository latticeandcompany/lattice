import { useEffect, useRef, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { shortenPath } from '../lib/format.ts';

// The open repo, and every other one you have opened, as one control near the top of
// the rail. A React-controlled dropdown, matching how the theme control works rather
// than pulling in Bootstrap's JS for one menu.
//
// The trigger is the project block itself rather than an icon beside it: swapping repos
// is the thing a person does most often in a window that only ever shows one, and an
// icon in the footer said nothing about what it switched.
const ProjectSwitcher = () => {
	const { recents, project, openProject, pickAndOpen, forget, close, busy } = useApp();
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		if (!open) return;
		const onDown = (event: MouseEvent) => {
			if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false);
		};
		const onKey = (event: KeyboardEvent) => {
			if (event.key === 'Escape') setOpen(false);
		};
		document.addEventListener('mousedown', onDown);
		document.addEventListener('keydown', onKey);
		return () => {
			document.removeEventListener('mousedown', onDown);
			document.removeEventListener('keydown', onKey);
		};
	}, [open]);

	const run = (work: () => void) => {
		setOpen(false);
		work();
	};

	// The one already open is not a place to go, so it is shown as current and the rest
	// of the list is what is actually offered.
	const others = recents.filter((recent) => recent.root !== project?.root);

	return (
		<div className="dropdown rail-project" ref={ref}>
			<button
				type="button"
				className="rail-project__trigger"
				onClick={() => setOpen((value) => !value)}
				aria-haspopup="menu"
				aria-expanded={open}
				aria-label={project ? `Repo: ${project.name}. Switch repo` : 'Open a repo'}
				disabled={busy}
			>
				<span className="tw:min-w-0 flex-grow-1">
					{project ? (
						<>
							<span className="rail-project__name">{project.name}</span>
							<span className="rail-project__path" title={project.root}>
								{shortenPath(project.root, 2)}
							</span>
						</>
					) : (
						<span className="rail-project__name">Open a repo…</span>
					)}
				</span>
				<i className="bi bi-chevron-expand rail-project__caret" aria-hidden="true" />
			</button>

			<ul className={`dropdown-menu rail-project__menu${open ? ' show' : ''}`}>
				{project && (
					<li>
						<span className="dropdown-item-text rail-project__current">
							<i className="bi bi-check2 me-2" aria-hidden="true" />
							{project.name}
						</span>
					</li>
				)}

				{others.length === 0 ? (
					<li>
						<span className="dropdown-item-text text-body-secondary small">
							{recents.length === 0 ? 'No repos opened yet.' : 'No other repos opened yet.'}
						</span>
					</li>
				) : (
					others.map((recent) => (
						<li key={recent.root} className="rail-project__row">
							<button
								type="button"
								className="dropdown-item"
								title={recent.name}
								onClick={() => run(() => void openProject(recent.root))}
							>
								<span className="rail-project__name">{recent.name}</span>
								<span className="rail-project__path" title={recent.root}>
									{shortenPath(recent.root)}
								</span>
							</button>
							<button
								type="button"
								className="rail-project__forget"
								title={`Forget ${recent.name}`}
								aria-label={`Forget ${recent.name}`}
								onClick={() => void forget(recent.root)}
							>
								<i className="bi bi-x" aria-hidden="true" />
							</button>
						</li>
					))
				)}

				<li>
					<hr className="dropdown-divider" />
				</li>
				<li>
					<button
						type="button"
						className="dropdown-item d-flex align-items-center gap-2"
						onClick={() => run(() => void pickAndOpen())}
					>
						<i className="bi bi-folder2-open" aria-hidden="true" />
						<span>Open another folder…</span>
					</button>
				</li>
				{project && (
					<li>
						<button
							type="button"
							className="dropdown-item d-flex align-items-center gap-2"
							onClick={() => run(() => void close())}
						>
							<i className="bi bi-x-circle" aria-hidden="true" />
							<span>Close this repo</span>
						</button>
					</li>
				)}
			</ul>
		</div>
	);
};

export default ProjectSwitcher;
