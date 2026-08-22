import { useEffect, useState } from 'react';
import CopyCommand from './copyCommand';

interface PlatformCommandProps {
	posix: string;
	windows: string;
	size?: 'sm' | 'md';
}

// navigator.platform is deprecated but still the only thing every browser answers,
// so it stands behind userAgentData rather than instead of it.
const onWindowsNow = () => {
	if (typeof navigator === 'undefined') {
		return false;
	}
	const hints = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
	if (hints?.platform) {
		return hints.platform === 'Windows';
	}
	return /win/i.test(navigator.platform || navigator.userAgent);
};

// One field, the command for the visitor's OS in it. The detection is a guess, so
// it is stated and reversible rather than silent: a Windows visitor who actually
// wants the POSIX line is one click away, and so is the reverse.
//
// The first render is the POSIX command on every platform. Detecting during render
// would make the server's HTML and React's first commit disagree, so the correction
// happens in an effect, after mount.
const PlatformCommand = ({ posix, windows, size = 'md' }: PlatformCommandProps) => {
	const [isWindows, setIsWindows] = useState(false);

	useEffect(() => {
		setIsWindows(onWindowsNow());
	}, []);

	const other = isWindows ? 'macOS and Linux' : 'Windows';

	return (
		<div className="d-flex flex-column align-items-center">
			<CopyCommand
				command={isWindows ? windows : posix}
				prompt={isWindows ? '>' : '$'}
				size={size}
			/>
			<p className="command-tab__hint mt-2 mb-0">
				{isWindows ? 'PowerShell' : 'macOS and Linux'}
				{' · '}
				<button
					type="button"
					className="btn btn-link p-0 align-baseline"
					style={{ fontSize: 'inherit' }}
					onClick={() => setIsWindows(!isWindows)}
				>
					show {other}
				</button>
			</p>
		</div>
	);
};

export default PlatformCommand;
