import { useEffect, useRef, useState } from 'react';

interface FieldHintProps {
	/** The sentence or two this control needs, but not on screen by default. */
	children: string;
	/** What the hint is about, so the trigger has a name of its own. */
	label: string;
}

// A Bootstrap popover driven by React rather than by Bootstrap's JS, the same way the
// theme and project menus are built. Bootstrap's popover styling is plain CSS; only
// the positioning needs Popper, and a hint that always opens directly under its own
// trigger does not need Popper to work that out.
//
// The prose is here rather than in a `.form-text` under every control because a form
// where each field carries a paragraph is a page of paragraphs with inputs in it.
const FieldHint = ({ children, label }: FieldHintProps) => {
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLSpanElement>(null);

	useEffect(() => {
		if (!open) return;
		const onDown = (event: MouseEvent) => {
			if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false);
		};
		const onKey = (event: KeyboardEvent) => {
			if (event.key === 'Escape') setOpen(false);
		};
		document.addEventListener('mousedown', onDown);
		document.addEventListener('keydown', onKey);
		return () => {
			document.removeEventListener('mousedown', onDown);
			document.removeEventListener('keydown', onKey);
		};
	}, [open]);

	return (
		<span className="position-relative d-inline-block" ref={ref}>
			<button
				type="button"
				className="btn btn-link btn-sm p-0 text-body-secondary d-inline-flex"
				onClick={() => setOpen((value) => !value)}
				aria-expanded={open}
				aria-label={`What ${label} does`}
			>
				<i className="bi bi-info-circle" aria-hidden="true" />
			</button>
			{open && (
				<span
					className="popover d-block position-absolute top-100 start-0 mt-1 tw:w-[20rem] tw:z-20"
					role="tooltip"
				>
					<span className="popover-body d-block">{children}</span>
				</span>
			)}
		</span>
	);
};

export default FieldHint;
