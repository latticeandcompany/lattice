import { LEGEND } from '../lib/graphOption.ts';

// Every encoding the graph uses gets a key. "Never rely on colour alone" generalises
// to "never rely on one channel alone", and a shape needs explaining as much as a hue.
const GraphLegend = () => (
	<div className="graph-legend">
		{LEGEND.map((entry) => (
			<span className="graph-legend__item" key={entry.label}>
				<i className={`bi ${entry.icon}`} aria-hidden="true" />
				<span>{entry.label}</span>
			</span>
		))}
	</div>
);

export default GraphLegend;
