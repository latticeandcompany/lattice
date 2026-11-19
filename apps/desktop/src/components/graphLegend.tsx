import { DECLARED_LEGEND, LEGEND } from '../lib/graphOption.ts';

interface GraphLegendProps {
	/** The graph has an edge lattice.json declares, so that line style needs a name. */
	declared?: boolean;
}

const GraphLegend = ({ declared = false }: GraphLegendProps) => (
	<ul className="graph-legend">
		{[...LEGEND, ...(declared ? [DECLARED_LEGEND] : [])].map((entry) => (
			<li key={entry.label}>
				<i className={`bi ${entry.icon}`} aria-hidden="true" />
				{entry.label}
			</li>
		))}
	</ul>
);

export default GraphLegend;
