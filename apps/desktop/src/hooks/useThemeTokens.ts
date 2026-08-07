// The brand tokens as literal colours, for the canvas that cannot read CSS.

import { useEffect, useState } from 'react';

import { observeTokens, readTokens, type Tokens } from '../lib/themeTokens.ts';

export const useThemeTokens = (): Tokens => {
	const [tokens, setTokens] = useState<Tokens>(() => readTokens());
	useEffect(() => observeTokens(setTokens), []);
	return tokens;
};

/** Whether the app is in dark mode right now, for picking an image variant. */
export const useIsDark = (): boolean => useThemeTokens().dark;
