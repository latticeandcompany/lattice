import { useEffect, useRef } from 'react';

import { useTaskOutputRev } from '../hooks/useRunStore.ts';
import { nothingToDraw, paneDraw } from '../lib/paneDraw.ts';
import { runStore } from '../lib/runStore.ts';

interface OutputPaneProps {
	taskKey: string;
}

// Output text never enters React.
//
// The element is rendered with no JSX children, so React does not reconcile what is
// inside it and appending is safe. That turns "ten thousand lines" from a
// reconciliation problem into an appendChild, which is the difference between a pane
// that scrolls and one that stutters.
const OutputPane = ({ taskKey }: OutputPaneProps) => {
	const ref = useRef<HTMLPreElement>(null);
	const drawn = useRef(0);
	const revision = useTaskOutputRev(taskKey);

	useEffect(() => {
		const element = ref.current;
		if (!element) return;

		const draw = paneDraw(element.childElementCount, runStore.linesSince(taskKey, drawn.current));
		if (nothingToDraw(draw)) return;

		// Stay pinned to the bottom unless the reader scrolled up to look at something.
		const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 24;

		const fragment = document.createDocumentFragment();
		for (const line of draw.append) {
			const span = document.createElement('span');
			span.className = line.stderr
				? 'code-card__line code-card__line--err'
				: 'code-card__line';
			span.textContent = line.line;
			fragment.append(span);
		}
		element.append(fragment);
		// The buffer drops its oldest past capacity, so the DOM and the buffer stay the
		// same length.
		for (let i = 0; i < draw.trim && element.firstChild; i += 1) {
			element.firstChild.remove();
		}
		drawn.current = draw.mark;

		if (atBottom) element.scrollTop = element.scrollHeight;
	}, [revision, taskKey]);

	return (
		<pre
			ref={ref}
			className="code-card__body code-card__body--log mb-0 selectable"
			tabIndex={0}
			aria-label={`Output for ${taskKey}`}
		/>
	);
};

export default OutputPane;
