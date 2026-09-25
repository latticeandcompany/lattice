import { useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { message } from '../lib/guard.ts';
import { declaredGroups, engineSource, nameList } from '../lib/toolchains.ts';
import type { ToolchainGroup } from '../lib/types.ts';
import PageHead from './pageHead.tsx';
import Spinner from './spinner.tsx';

// The tools this project pins, grouped by the workspaces that pin the same ones.
//
// Before a check this is what lattice.json asks for. A check does exactly what a run
// does first — reads each version, installs what has an installCmd and has not been
// installed — and then says what it found and where, or prints the failure the way
// the CLI prints it.
const ToolchainsView = () => {
	const { project, setView } = useApp();
	const [checked, setChecked] = useState<ToolchainGroup[] | null>(null);
	const [checking, setChecking] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const declared = useMemo(() => declaredGroups(project?.workspaces ?? []), [project]);
	const groups = checked ?? declared;

	if (!project) return null;

	const check = async () => {
		setChecking(true);
		setError(null);
		try {
			setChecked(await api.toolchainsCheck());
		} catch (caught) {
			setError(message(caught));
		} finally {
			setChecking(false);
		}
	};

	const failures = checked?.filter((group) => group.error !== null).length ?? 0;

	return (
		<>
			<PageHead title="Toolchains">
				{checked && (
					<span className="small text-body-secondary">
						{failures === 0 ? 'Every pin checks out' : `${failures} ${failures === 1 ? 'group fails' : 'groups fail'}`}
					</span>
				)}
				<button
					type="button"
					className="btn btn-primary btn-sm d-inline-flex align-items-center gap-2"
					onClick={() => void check()}
					disabled={checking || groups.length === 0}
				>
					{checking && <Spinner label="Checking" quiet />}
					{checking ? 'Checking…' : checked ? 'Check again' : 'Check versions'}
				</button>
			</PageHead>

			<div className="app-scroll">
				<div className="app-column">
					{error && <pre className="cli-error selectable mb-3">{error}</pre>}

					{groups.length === 0 ? (
						<div className="empty-state">
							<h2>Nothing is pinned</h2>
							<p>
								Every task uses whatever version of each tool is on this machine. Pin one under engines in the
								config, and every run checks it before a task starts.
							</p>
							<button type="button" className="btn btn-outline-secondary btn-sm" onClick={() => setView('config')}>
								Open the config
							</button>
						</div>
					) : (
						groups.map((group) => (
							<section className="tool-group" key={group.workspaces.join(',')}>
								<h2 title={group.workspaces.join(', ')}>{nameList(group.workspaces)}</h2>
								<table className="table table-sm">
									<thead>
										<tr>
											<th scope="col">Tool</th>
											<th scope="col">Wants</th>
											<th scope="col">Found</th>
											<th scope="col">From</th>
										</tr>
									</thead>
									<tbody>
										{group.engines.map((engine) => (
											<tr key={engine.name}>
												<td className="fw-medium">{engine.name}</td>
												<td className="font-monospace small">{engine.wanted ?? 'any version'}</td>
												<td>
													<span className="font-monospace small">{engine.version ?? '—'}</span>
													{engine.location && (
														<span className="tool-location selectable" title={engine.location}>
															{engine.location}
														</span>
													)}
												</td>
												<td className="small text-body-secondary">{engineSource(engine, checked !== null)}</td>
											</tr>
										))}
									</tbody>
								</table>
								{group.error && <pre className="cli-error selectable">{group.error}</pre>}
							</section>
						))
					)}
				</div>
			</div>
		</>
	);
};

export default ToolchainsView;
