import { useCallback, useEffect, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import type { EnginePin, ScanResult, WorkspaceCandidate } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import Spinner from './spinner.tsx';

// The numbered rail from the website's get-started page, because the steps are a
// sequence and the connecting thread is what makes them read as one.
//
// The preview comes from the backend, not from assembling JSON here. `lattice init`
// encodes decisions — when a dist/** output is justified, when engines is omitted
// entirely — and a second renderer in TypeScript would drift from it on the first
// change to either.
const SetupWizard = () => {
	const { pendingRoot, openProject, pickAndOpen, catalog } = useApp();

	const [scan, setScan] = useState<ScanResult | null>(null);
	const [chosenWorkspaces, setChosenWorkspaces] = useState<Set<string>>(new Set());
	const [chosenPins, setChosenPins] = useState<Set<string>>(new Set());
	const [preview, setPreview] = useState<string>('');
	const [error, setError] = useState<string | null>(null);
	const [writing, setWriting] = useState(false);

	const language = useCallback(
		(driver: string | null) =>
			catalog?.drivers.find((entry) => entry.tool === driver)?.language ?? null,
		[catalog],
	);

	useEffect(() => {
		if (!pendingRoot) return;
		let cancelled = false;
		void (async () => {
			try {
				const result = await api.configScan(pendingRoot);
				if (cancelled) return;
				setScan(result);
				// Honour the engine's own judgement about what to propose: a candidate
				// with no resolved driver is offered but not pre-selected.
				setChosenWorkspaces(
					new Set(result.candidates.filter((c) => c.defaultSelected).map((c) => c.path)),
				);
				setChosenPins(new Set(result.pins.map((pin) => pin.engine)));
			} catch (caught) {
				if (!cancelled) setError(caught instanceof Error ? caught.message : String(caught));
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [pendingRoot]);

	const selection = useCallback(
		(): api.InitSelection => ({
			candidates: (scan?.candidates ?? []).filter((c) => chosenWorkspaces.has(c.path)),
			pins: (scan?.pins ?? []).filter((pin) => chosenPins.has(pin.engine)),
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
				if (!cancelled) setError(caught instanceof Error ? caught.message : String(caught));
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
			setError(caught instanceof Error ? caught.message : String(caught));
		} finally {
			setWriting(false);
		}
	};

	if (!pendingRoot) {
		return (
			<div className="app-main__scroll">
				<div className="empty-state">
					<i className="bi bi-folder2-open fs-1" aria-hidden="true" />
					<h1 className="h4 fw-bold" style={{ color: 'var(--text)' }}>
						Open a repo to get started
					</h1>
					<p style={{ maxWidth: '28rem' }}>
						Pick a folder. If it already has a lattice.json, Lattice opens it. If it does not,
						Lattice looks around the repo and suggests one.
					</p>
					<button type="button" className="btn btn-primary px-4" onClick={() => void pickAndOpen()}>
						Choose a folder
					</button>
				</div>
			</div>
		);
	}

	return (
		<div className="app-main__scroll">
			<div className="app-main__inner" style={{ maxWidth: '48rem' }}>
				<h1 className="fw-bold mb-4" style={{ fontSize: '2rem', letterSpacing: '-0.02em' }}>
					Set up this repo
				</h1>

				{error && (
					<div className="notice notice--bad mb-3">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div className="selectable">{error}</div>
					</div>
				)}

				<div className="step">
					<div className="step__num">1</div>
					<div className="step__body">
						<h2 className="step__title">The folder</h2>
						<div className="step__code step__code--file selectable">{pendingRoot}</div>
						<button
							type="button"
							className="btn btn-outline-secondary btn-sm"
							onClick={() => void pickAndOpen()}
						>
							Choose a different one
						</button>
					</div>
				</div>

				<div className="step">
					<div className="step__num">2</div>
					<div className="step__body">
						<h2 className="step__title">Where things can run</h2>
						{scan === null ? (
							<Spinner label="Looking through the repo…" className="step__hint" />
						) : scan.candidates.length === 0 ? (
							<p className="step__hint">
								Nothing here looks like a workspace yet — no package.json, Cargo.toml, go.mod, or
								anything else Lattice knows. You can still add folders by hand once the config
								exists.
							</p>
						) : (
							<>
								<p className="step__hint">
									Every folder with a manifest of its own is listed. The ones Lattice could not
									find a tool for start unticked, because there would be nothing to run in them
									yet.
								</p>
								{scan.candidates.map((candidate) => (
									<CandidateRow
										key={candidate.path}
										candidate={candidate}
										language={language(candidate.driver)}
										checked={chosenWorkspaces.has(candidate.path)}
										onToggle={() =>
											setChosenWorkspaces((current) => toggle(current, candidate.path))
										}
									/>
								))}
							</>
						)}
					</div>
				</div>

				<div className="step">
					<div className="step__num">3</div>
					<div className="step__body">
						<h2 className="step__title">Tool versions to carry over</h2>
						{scan === null ? (
							<Spinner label="Looking for pinned versions…" className="step__hint" />
						) : scan.pins.length === 0 ? (
							<p className="step__hint">
								Nothing in this repo records a tool version yet, so there is nothing to carry over.
							</p>
						) : (
							<>
								<p className="step__hint">
									Copied exactly as the files here already have them. Nothing is being changed.
								</p>
								{scan.pins.map((pin) => (
									<PinRow
										key={pin.engine}
										pin={pin}
										checked={chosenPins.has(pin.engine)}
										onToggle={() => setChosenPins((current) => toggle(current, pin.engine))}
									/>
								))}
							</>
						)}
					</div>
				</div>

				<div className="step">
					<div className="step__num">4</div>
					<div className="step__body">
						<h2 className="step__title">What gets written</h2>
						<div className="card code-card mb-3">
							<div className="card-header code-card__bar">
								<span className="code-card__dot" />
								<span className="code-card__dot" />
								<span className="code-card__dot" />
								<span className="code-card__name">lattice.json</span>
							</div>
							<div className="card-body p-0">
								<pre className="code-card__body mb-0 selectable">{preview}</pre>
							</div>
						</div>
						<p className="step__hint mb-3">
							Two other things get written: <code>.lattice/schema.json</code>, so your editor can
							check the config as you type, and three lines added to <code>.gitignore</code> to keep
							the cache, the tools Lattice fetches, and the binaries it installs out of git.
						</p>
						<button
							type="button"
							className="btn btn-primary px-4 d-inline-flex align-items-center gap-2"
							onClick={() => void write()}
							disabled={writing || scan === null}
						>
							{writing && <Spinner label="Writing" quiet />}
							{writing ? 'Writing…' : 'Write lattice.json'}
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
	<div className="scan-row">
		<input
			className="form-check-input mt-0"
			type="checkbox"
			checked={checked}
			onChange={onToggle}
			id={`candidate-${candidate.path}`}
			aria-label={`Include ${candidate.name}`}
		/>
		<LanguageMark tool={candidate.driver} language={language} />
		<label className="flex-grow-1 tw:min-w-0" htmlFor={`candidate-${candidate.path}`}>
			<span className="d-block" style={{ fontWeight: 500 }}>
				{candidate.name}
			</span>
			<span className="scan-row__meta d-block">
				{candidate.path} · found {candidate.marker}
				{candidate.driver ? ` · runs with ${candidate.driver}` : ' · nothing found to run it with'}
			</span>
		</label>
	</div>
);

const PinRow = ({
	pin,
	checked,
	onToggle,
}: {
	pin: EnginePin;
	checked: boolean;
	onToggle: () => void;
}) => (
	<div className="scan-row">
		<input
			className="form-check-input mt-0"
			type="checkbox"
			checked={checked}
			onChange={onToggle}
			id={`pin-${pin.engine}`}
			aria-label={`Pin ${pin.engine} at ${pin.version}`}
		/>
		<label className="flex-grow-1" htmlFor={`pin-${pin.engine}`}>
			<span className="chip me-2">
				{pin.engine} {pin.version}
			</span>
			<span className="scan-row__meta">already written in {pin.source}</span>
		</label>
	</div>
);

export default SetupWizard;
