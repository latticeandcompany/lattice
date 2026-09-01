import { useEffect, useState } from 'react';
import { downloadUrl, platforms, type OsKey, type Platform } from '../lib/desktop.ts';

interface Guess {
	os: OsKey;
	arch: string;
}

const FALLBACK: Guess = { os: 'macos', arch: 'aarch64' };

interface UaData {
	platform?: string;
	getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }>;
}

const uaData = (): UaData | undefined =>
	typeof navigator === 'undefined' ? undefined : (navigator as Navigator & { userAgentData?: UaData }).userAgentData;

const osFromUa = (): OsKey | undefined => {
	if (typeof navigator === 'undefined') return undefined;
	const hinted = uaData()?.platform;
	if (hinted === 'macOS') return 'macos';
	if (hinted === 'Windows') return 'windows';
	if (hinted === 'Linux') return 'linux';

	// navigator.platform is deprecated, but it is still the only field every
	// browser answers. So it runs after the hints, not instead of them.
	const raw = `${navigator.platform || ''} ${navigator.userAgent || ''}`;
	if (/mac/i.test(raw)) return 'macos';
	if (/win/i.test(raw)) return 'windows';
	if (/linux|x11|cros/i.test(raw)) return 'linux';
	return undefined;
};

/**
 * A guess, refined once if the browser will say more.
 *
 * Only Chromium answers `getHighEntropyValues`. No browser reports Apple Silicon
 * at all, because Safari names an Intel Mac whatever the hardware is. So macOS
 * defaults to Apple Silicon, which covers every Mac sold since 2020. The page
 * states the guess and puts every other build one click away.
 */
const useGuess = (): Guess => {
	const [guess, setGuess] = useState<Guess>(FALLBACK);

	useEffect(() => {
		const os = osFromUa();
		if (!os) return;
		setGuess({ os, arch: os === 'macos' ? 'aarch64' : 'x86_64' });

		if (os === 'macos') return;
		uaData()
			?.getHighEntropyValues?.(['architecture'])
			.then(({ architecture }) => {
				if (architecture === 'arm') setGuess({ os, arch: 'aarch64' });
			})
			.catch(() => {});
	}, []);

	return guess;
};

const Primary = ({ platform }: { platform: Platform }) => {
	const download = platform.downloads[0];
	return (
		<a className="btn btn-desktop btn-lg px-4 download-primary" href={downloadUrl(download.asset)}>
			<i className={`bi ${platform.icon} me-2`} aria-hidden="true" />
			Download for {platform.shortLabel}
		</a>
	);
};

const DownloadPicker = () => {
	const guess = useGuess();
	const [chosen, setChosen] = useState<string | null>(null);

	const key = (p: Platform) => `${p.os}-${p.arch}`;
	const guessed = platforms.find((p) => p.os === guess.os && p.arch === guess.arch) ?? platforms[0];
	const platform = platforms.find((p) => key(p) === chosen) ?? guessed;
	const others = platforms.filter((p) => key(p) !== key(platform));

	return (
		<div className="d-flex flex-column align-items-center">
			<Primary platform={platform} />
			<p className="download-picker__hint mt-3 mb-1">Showing {platform.label}. Other builds:</p>
			<p className="download-picker__hint mb-0">
				{others.map((p, i) => (
					<span key={key(p)}>
						{i > 0 && ' · '}
						<button
							type="button"
							className="btn btn-link p-0 align-baseline"
							style={{ fontSize: 'inherit' }}
							onClick={() => setChosen(key(p))}
						>
							{p.shortLabel}
						</button>
					</span>
				))}
			</p>
		</div>
	);
};

export default DownloadPicker;
