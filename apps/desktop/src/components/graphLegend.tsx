import { LEGEND } from '../lib/graphOption.ts';

const GraphLegend = () => (
	<div className="d-flex flex-wrap gap-2">
		{LEGEND.map((entry) => (
			<span
				className="badge border bg-body-tertiary text-body-secondary fw-normal d-inline-flex align-items-center gap-1"
				key={entry.label}
			>
				<i className={`bi ${entry.icon}`} aria-hidden="true" />
				{entry.label}
			</span>
		))}
	</div>
);

export default GraphLegend;
