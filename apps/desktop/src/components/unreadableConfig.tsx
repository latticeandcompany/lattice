import { useApp } from '../context/appContext.tsx';

// A lattice.json that exists and does not load. The project cannot be shown until
// it does, so this says why, in the parser's own words, and waits for a reload.
const UnreadableConfig = () => {
	const { diagnostics, reload, pickAndOpen, busy } = useApp();

	return (
		<div className="app-scroll">
			<div className="app-column app-column--narrow">
				<div className="empty-state">
					<h2>This lattice.json does not load</h2>
					<p>Fix it in your editor, then reload. Lattice reads it the same way a run would.</p>
				</div>
				{diagnostics.map((diagnostic, index) => (
					<pre className="cli-error selectable" key={index}>
						{diagnostic.message}
					</pre>
				))}
				<div className="d-flex gap-2 mt-3">
					<button type="button" className="btn btn-primary" onClick={() => void reload()} disabled={busy}>
						Reload
					</button>
					<button type="button" className="btn btn-outline-secondary" onClick={() => void pickAndOpen()}>
						Open another project…
					</button>
				</div>
			</div>
		</div>
	);
};

export default UnreadableConfig;
