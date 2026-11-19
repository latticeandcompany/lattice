import { useEffect, useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { message } from '../lib/guard.ts';
import { lineCounts, lineDiff } from '../lib/lineDiff.ts';
import type { MigrationPreview } from '../lib/types.ts';
import PageHead from './pageHead.tsx';
import Spinner from './spinner.tsx';

// A lattice.json written for Lattice 1.x does not load in this version. Lattice
// works out what the repo's own manifests already say and writes only the rest, so
// this shows that rewrite as a diff before anything is written.
const MigrateView = () => {
	const { project, pendingRoot, migrate, busy, pickAndOpen } = useApp();
	const root = project?.root ?? pendingRoot;
	const [preview, setPreview] = useState<MigrationPreview | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		if (!root) return;
		let cancelled = false;
		void (async () => {
			try {
				const next = await api.migratePreview(root);
				if (!cancelled) setPreview(next);
			} catch (caught) {
				if (!cancelled) setError(message(caught));
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [root]);

	const diff = useMemo(() => (preview ? lineDiff(preview.before, preview.after) : []), [preview]);
	const counts = preview ? lineCounts(preview.before, preview.after) : null;

	return (
		<>
			<PageHead title="Update lattice.json for this version">
				<button
					type="button"
					className="btn btn-primary btn-sm d-inline-flex align-items-center gap-2"
					onClick={() => void migrate()}
					disabled={busy || preview === null}
				>
					{busy && <Spinner label="Writing" quiet />}
					Rewrite lattice.json
				</button>
			</PageHead>

			<div className="app-scroll">
				<div className="app-column">
					<p className="step__hint">
						This config lists every workspace by name and path, the way Lattice 1.x needed. This version reads
						workspaces and the order between them from package.json, Cargo.toml, go.mod and the other manifests, so
						the rewrite keeps only what they cannot say. Every workspace keeps its name.
					</p>

					{error && (
						<>
							<pre className="cli-error selectable">{error}</pre>
							<button type="button" className="btn btn-outline-secondary btn-sm mt-3" onClick={() => void pickAndOpen()}>
								Open another project…
							</button>
						</>
					)}

					{preview === null && !error && <Spinner label="Working out the rewrite…" />}

					{preview && counts && (
						<>
							<h2 className="mt-4">
								{counts.before} lines become {counts.after}
							</h2>
							<div className="diff selectable" role="region" aria-label="Changes to lattice.json">
								{diff.map((line, index) => (
									<span
										key={index}
										className={`diff__line diff__line--${line.kind}`}
										data-mark={line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ''}
									>
										{line.text || ' '}
									</span>
								))}
							</div>

							{preview.notes.length > 0 && (
								<>
									<h2>What changed that the diff does not show</h2>
									<ul className="notes-list selectable">
										{preview.notes.map((note) => (
											<li key={note}>{note}</li>
										))}
									</ul>
								</>
							)}
						</>
					)}
				</div>
			</div>
		</>
	);
};

export default MigrateView;
