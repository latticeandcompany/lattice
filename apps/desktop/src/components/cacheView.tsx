import { useCallback, useEffect, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useRunView } from '../hooks/useRunStore.ts';
import * as api from '../lib/api.ts';
import { fmtBytes, fmtCount, fmtDay, fmtSpan, hitRate } from '../lib/format.ts';
import { message } from '../lib/guard.ts';
import type { CacheReport } from '../lib/types.ts';
import PageHead from './pageHead.tsx';

type Pending = 'limit' | 'empty' | null;

// What the cache holds and what it has saved. Pruning is here because this is where
// its size is visible; emptying asks first, since the next run then does everything.
const CacheView = () => {
	const { project, setView } = useApp();
	const run = useRunView();
	const [report, setReport] = useState<CacheReport | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [confirm, setConfirm] = useState<Pending>(null);
	const [freed, setFreed] = useState<string | null>(null);
	const [working, setWorking] = useState(false);

	const load = useCallback(async () => {
		try {
			setReport(await api.cacheReport());
			setError(null);
		} catch (caught) {
			setError(message(caught));
		}
	}, []);

	// A finished run changes the numbers, so re-read when one settles.
	useEffect(() => {
		void load();
	}, [load, project?.root, run.outcome]);

	if (!project) return null;

	const prune = async (maxBytes: number) => {
		setWorking(true);
		try {
			const result = await api.cachePrune(maxBytes);
			setFreed(
				result.removed === 0
					? 'Nothing needed removing.'
					: `Removed ${fmtCount(result.removed)} ${result.removed === 1 ? 'entry' : 'entries'}, freed ${fmtBytes(result.bytesFreed)}.`,
			);
			setConfirm(null);
			await load();
		} catch (caught) {
			setError(message(caught));
		} finally {
			setWorking(false);
		}
	};

	const share = report?.maxBytes ? Math.min(100, (report.bytes / report.maxBytes) * 100) : null;
	const rate = report ? hitRate(report.hits, report.tasks) : null;

	return (
		<>
			<PageHead title="Cache">{report && <span className="datum">{report.dir}</span>}</PageHead>

			<div className="app-scroll">
				<div className="app-column app-column--narrow">
					{error && <pre className="cli-error selectable mb-3">{error}</pre>}

					{report && (
						<>
							<div className="cache-gauge">
								<span className="cache-gauge__size">{fmtBytes(report.bytes)}</span>
								<span className="cache-gauge__of">
									{report.maxBytes ? `of ${fmtBytes(report.maxBytes)}` : 'with no size limit'}
								</span>
								{share !== null && (
									<div
										className="progress"
										role="progressbar"
										aria-label="Cache size against its limit"
										aria-valuenow={Math.round(share)}
										aria-valuemin={0}
										aria-valuemax={100}
									>
										<div className="progress-bar" style={{ width: `${share}%` }} />
									</div>
								)}
							</div>

							{report.runs === 0 ? (
								<p className="text-body-secondary">No runs recorded yet. Each run adds to what is shown here.</p>
							) : (
								<dl className="facts mb-4">
									<dt>Time saved</dt>
									<dd>
										<span className="fact-number">{fmtSpan(report.savedMs)}</span>
										<span className="fact-aside">
											{fmtSpan(report.recentSavedMs)} in the last 7 days
										</span>
									</dd>
									<dt>Runs</dt>
									<dd>
										{fmtCount(report.runs)}
										{report.since && <span className="fact-aside">since {fmtDay(report.since)}</span>}
									</dd>
									<dt>Tasks from cache</dt>
									<dd>
										{fmtCount(report.hits)} of {fmtCount(report.tasks)}
										{rate !== null && <span className="fact-aside">{rate}%</span>}
									</dd>
									<dt>Entries</dt>
									<dd>{fmtCount(report.entries)}</dd>
								</dl>
							)}

							{freed && (
								<p className="small" role="status">
									{freed}
								</p>
							)}

							{confirm === null ? (
								<div className="d-flex flex-wrap gap-2">
									{report.maxBytes !== null && (
										<button
											type="button"
											className="btn btn-outline-secondary btn-sm"
											onClick={() => void prune(report.maxBytes ?? 0)}
											disabled={working || report.entries === 0}
										>
											Prune to {fmtBytes(report.maxBytes)}
										</button>
									)}
									<button
										type="button"
										className="btn btn-outline-secondary btn-sm"
										onClick={() => setConfirm('empty')}
										disabled={working || report.entries === 0}
									>
										Empty the cache
									</button>
									{report.maxBytes === null && (
										<button type="button" className="btn btn-link btn-sm" onClick={() => setView('config')}>
											Set a size limit
										</button>
									)}
								</div>
							) : (
								<div className="confirm-line" role="alertdialog" aria-label="Empty the cache">
									<span>
										Delete all {fmtCount(report.entries)} entries? The next run of every task runs it again.
									</span>
									<button
										type="button"
										className="btn btn-primary btn-sm"
										onClick={() => void prune(0)}
										disabled={working}
									>
										Empty the cache
									</button>
									<button type="button" className="btn btn-outline-secondary btn-sm" onClick={() => setConfirm(null)}>
										Keep it
									</button>
								</div>
							)}
						</>
					)}
				</div>
			</div>
		</>
	);
};

export default CacheView;
