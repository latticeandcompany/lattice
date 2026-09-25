import { useCallback, useEffect, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { message } from '../lib/guard.ts';
import type { EnginePin, InitSelection, ScanResult, WorkspaceCandidate } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import Rosette from './rosette.tsx';
import Spinner from './spinner.tsx';

// Setting a project up. Most repos need very little written down: workspaces a
// workspace file already names, and every task a manifest defines, are found
// without it. So the wizard shows those as found and asks only about the rest.
//
// The preview comes from the backend, which is what `lattice init` uses, so a config
// written here is byte for byte the one the CLI would write.
const SetupWizard = () => {
	const { pendingRoot, openProject, pickAndOpen, catalog } = useApp();

	const [scan, setScan] = useState<ScanResult | null>(null);
	const [chosenWorkspaces, setChosenWorkspaces] = useState<Set<string>>(new Set());
	const [chosenPins, setChosenPins] = useState<Set<string>>(new Set());
	const [preview, setPreview] = useState('');
	const [error, setError] = useState<string | null>(null);
	const [writing, setWriting] = useState(false);

	const language = useCallback(
		(driver: string | null) => catalog?.drivers.find((entry) => entry.tool === driver)?.language ?? null,
		[catalog],
	);

	useEffect(() => {
		if (!pendingRoot) return;
		let cancelled = false;
		setScan(null);
		void (async () => {
			try {
				const result = await api.configScan(pendingRoot);
				if (cancelled) return;
				setScan(result);
				// A candidate with no resolved driver is offered but not pre-selected.
				setChosenWorkspaces(new Set(result.candidates.filter((c) => c.defaultSelected).map((c) => c.path)));
				setChosenPins(new Set(result.pins.map((pin) => pin.engine)));
			} catch (caught) {
				if (!cancelled) setError(message(caught));
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [pendingRoot]);

	const selection = useCallback(
		(): InitSelection => ({
			candidates: (scan?.candidates ?? []).filter((c) => chosenWorkspaces.has(c.path)),
			pins: (scan?.pins ?? []).filter((pin) => chosenPins.has(pin.engine)),
			offered: (scan?.candidates ?? []).map((c) => c.path),
			rootSettings: scan?.rootSettings ?? null,
		}),
		[scan, chosenWorkspaces, chosenPins],
	);

	useEffect(() => {
		if (!scan) return;
		let cancelled = false;
		void (async () => {
			try {
				const text = await api.configPreview(selection());
				if (!cancelled) setPreview(text);
			} catch (caught) {
				if (!cancelled) setError(message(caught));
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [scan, selection]);

	const write = async () => {
		if (!pendingRoot) return;
		setWriting(true);
		setError(null);
		try {
			await api.configInit(pendingRoot, selection(), false);
			await openProject(pendingRoot);
		} catch (caught) {
			setError(message(caught));
		} finally {
			setWriting(false);
		}
	};

	if (!pendingRoot) {
		return (
			<div className="app-scroll">
				<div className="app-column app-column--narrow">
					<div className="empty-state">
						<Rosette size="3.5rem" />
						<h2 className="mt-4">Open a repo</h2>
						<p>
							Pick its root directory. If it has a lattice.json, Lattice opens it. If not, Lattice reads the repo and
							shows what a config needs to say.
						</p>
						<button type="button" className="btn btn-primary" onClick={() => void pickAndOpen()}>
							Open a repo…
						</button>
					</div>
				</div>
			</div>
		);
	}

	const extras = scan?.candidates ?? [];

	return (
		<div className="app-scroll">
			<div className="app-column app-column--narrow">
				<h1 className="mb-4">Set up this repo</h1>

				{error && <pre className="cli-error selectable mb-3">{error}</pre>}

				<div className="step">
					<div className="step__num">1</div>
					<div className="step__body">
						<h2>Where lattice.json goes</h2>
						<p className="step__hint font-monospace selectable">{pendingRoot}</p>
						<button type="button" className="btn btn-outline-secondary btn-sm" onClick={() => void pickAndOpen()}>
							Choose another directory
						</button>
					</div>
				</div>

				<div className="step">
					<div className="step__num">2</div>
					<div className="step__body">
						<h2>Workspaces</h2>
						{scan === null ? (
							<Spinner label="Reading the repo…" className="step__hint" />
						) : (
							<>
								{scan.native.length > 0 && (
									<>
										<p className="step__hint">
											Found through the repo's own workspace files. These need nothing in lattice.json.
										</p>
										<ul className="native-list">
											{scan.native.map((path) => (
												<li key={path}>{path}</li>
											))}
										</ul>
									</>
								)}
								{extras.length > 0 ? (
									<>
										<p className="step__hint">
											{scan.native.length > 0
												? 'No workspace file names these. Tick the ones that are workspaces.'
												: 'Projects Lattice found in the repo. Tick the ones that are workspaces.'}
										</p>
										{extras.map((candidate) => (
											<CandidateRow
												key={candidate.path}
												candidate={candidate}
												language={language(candidate.driver)}
												checked={chosenWorkspaces.has(candidate.path)}
												onToggle={() => setChosenWorkspaces((current) => toggle(current, candidate.path))}
											/>
										))}
									</>
								) : (
									scan.native.length === 0 && (
										<p className="step__hint">
											Lattice treats the root as the one workspace. Add others under workspaces later if the repo
											grows.
										</p>
									)
								)}
							</>
						)}
					</div>
				</div>

				{scan && scan.pins.length > 0 && (
					<div className="step">
						<div className="step__num">3</div>
						<div className="step__body">
							<h2>Tool versions</h2>
							<p className="step__hint">
								The repo already records these. Keep one ticked and every run checks it before a task starts.
							</p>
							{scan.pins.map((pin) => (
								<PinRow
									key={pin.engine}
									pin={pin}
									checked={chosenPins.has(pin.engine)}
									onToggle={() => setChosenPins((current) => toggle(current, pin.engine))}
								/>
							))}
						</div>
					</div>
				)}

				<div className="step">
					<div className="step__num">{scan && scan.pins.length > 0 ? 4 : 3}</div>
					<div className="step__body">
						<h2>lattice.json</h2>
						<pre className="file-preview selectable">{preview}</pre>
						<p className="step__hint">
							Lattice also writes .lattice/schema.json, so your editor checks the config as you type, and adds its
							cache and installed tools to .gitignore.
						</p>
						<button
							type="button"
							className="btn btn-primary d-inline-flex align-items-center gap-2"
							onClick={() => void write()}
							disabled={writing || scan === null}
						>
							{writing && <Spinner label="Writing" quiet />}
							Write lattice.json
						</button>
					</div>
				</div>
			</div>
		</div>
	);
};

const toggle = (current: Set<string>, value: string): Set<string> => {
	const next = new Set(current);
	if (next.has(value)) next.delete(value);
	else next.add(value);
	return next;
};

const CandidateRow = ({
	candidate,
	language,
	checked,
	onToggle,
}: {
	candidate: WorkspaceCandidate;
	language: string | null;
	checked: boolean;
	onToggle: () => void;
}) => (
	<div className="pick-row">
		<input
			className="form-check-input mt-0"
			type="checkbox"
			checked={checked}
			onChange={onToggle}
			id={`candidate-${candidate.path}`}
		/>
		<LanguageMark tool={candidate.driver} language={language} />
		<label htmlFor={`candidate-${candidate.path}`}>
			<span className="d-block font-monospace">{candidate.path}</span>
			<span className="d-block small text-body-secondary">
				{candidate.driver
					? `${candidate.driver}, from ${candidate.marker}`
					: `${candidate.marker}, but nothing says which tool runs it`}
			</span>
		</label>
	</div>
);

const PinRow = ({ pin, checked, onToggle }: { pin: EnginePin; checked: boolean; onToggle: () => void }) => (
	<div className="pick-row">
		<input className="form-check-input mt-0" type="checkbox" checked={checked} onChange={onToggle} id={`pin-${pin.engine}`} />
		<label htmlFor={`pin-${pin.engine}`}>
			<span className="fw-medium">{pin.engine}</span> <span className="font-monospace small">{pin.version}</span>
			<span className="d-block small text-body-secondary">recorded in {pin.source}</span>
		</label>
	</div>
);

export default SetupWizard;
