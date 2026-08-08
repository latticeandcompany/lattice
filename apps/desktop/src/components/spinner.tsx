interface SpinnerProps {
	/** What is happening. Read aloud, and shown beside the spinner unless `quiet`. */
	label: string;
	/** Keep the label for assistive tech but leave it off the screen. */
	quiet?: boolean;
	className?: string;
}

// Bootstrap's spinner, wrapped so it never ships without a word attached. Animation is
// not a state a screen reader can see, and it is not a state anyone can read in a
// screenshot either, so the label is required rather than optional.
const Spinner = ({ label, quiet = false, className }: SpinnerProps) => (
	<span
		className={['d-inline-flex align-items-center gap-2', className].filter(Boolean).join(' ')}
		role="status"
	>
		<span className="spinner-border spinner-border-sm" aria-hidden="true" />
		<span className={quiet ? 'visually-hidden' : undefined}>{label}</span>
	</span>
);

export default Spinner;
