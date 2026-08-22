import { useState } from 'react';
import CopyCommand from './copyCommand';

export interface CommandTab {
	name: string;
	command: string;
	/** One quiet line under the field, for a command whose label isn't self-explanatory. */
	hint?: string;
	/** Makes that line a link. Props cross an island boundary, so this stays a string. */
	hintHref?: string;
}

interface CommandTabsProps {
	tabs: CommandTab[];
	size?: 'sm' | 'md';
}

// Two commands, one field. The first tab is what most people came for; the rest sit
// behind a label rather than a second block of hero copy.
const CommandTabs = ({ tabs, size = 'md' }: CommandTabsProps) => {
	const [active, setActive] = useState(0);
	const tab = tabs[active];

	return (
		<div className="d-flex flex-column align-items-center">
			<div className="command-tabs mb-2" role="tablist" aria-label="What to install">
				{tabs.map((t, i) => (
					<button
						key={t.name}
						type="button"
						role="tab"
						aria-selected={i === active}
						className={`command-tab${i === active ? ' command-tab--active' : ''}`}
						onClick={() => setActive(i)}
					>
						{t.name}
					</button>
				))}
			</div>
			<CopyCommand command={tab.command} size={size} />
			{tab.hint && (
				<p className="command-tab__hint mt-2 mb-0">
					{tab.hintHref ? <a href={tab.hintHref}>{tab.hint}</a> : tab.hint}
				</p>
			)}
		</div>
	);
};

export default CommandTabs;
