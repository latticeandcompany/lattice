interface IconButtonProps {
	icon: string;
	/** Both the tooltip and the accessible name, since the glyph carries neither. */
	label: string;
	onClick: () => void;
	disabled?: boolean;
	expanded?: boolean;
	className?: string;
}

// A square, quiet button holding one glyph.
const IconButton = ({ icon, label, onClick, disabled = false, expanded, className }: IconButtonProps) => (
	<button
		type="button"
		className={['icon-btn', className].filter(Boolean).join(' ')}
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
