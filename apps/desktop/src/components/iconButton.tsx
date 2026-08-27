import type { ReactNode } from 'react';

interface IconButtonProps {
	icon: string;
	/** Both the tooltip and the accessible name, since the glyph carries neither. */
	label: string;
	onClick: () => void;
	disabled?: boolean;
	expanded?: boolean;
	className?: string;
	children?: ReactNode;
}

// A square, quiet button holding one glyph. Bootstrap's `.btn` sizing is built for a
// word, so a row of these needs the box squared off — that is the only thing the
// utilities here are doing.
const IconButton = ({
	icon,
	label,
	onClick,
	disabled = false,
	expanded,
	className,
}: IconButtonProps) => (
	<button
		type="button"
		className={[
			'btn btn-sm border text-body-secondary d-inline-flex align-items-center justify-content-center',
			'tw:h-[1.9rem] tw:w-[1.9rem] tw:p-0',
			className,
		]
			.filter(Boolean)
			.join(' ')}
		onClick={onClick}
		disabled={disabled}
		aria-expanded={expanded}
		title={label}
		aria-label={label}
	>
		<i className={`bi ${icon}`} aria-hidden="true" />
	</button>
);

export default IconButton;
