import { useEffect, useState } from 'react';

// The site ships a full reduced-motion block; the graph has to honour the same
// preference, and a canvas animation is not something CSS can switch off.
export const useReducedMotion = (): boolean => {
	const [reduced, setReduced] = useState(
		() =>
			typeof window !== 'undefined' &&
			window.matchMedia('(prefers-reduced-motion: reduce)').matches,
	);

	useEffect(() => {
		const query = window.matchMedia('(prefers-reduced-motion: reduce)');
		const onChange = () => setReduced(query.matches);
		query.addEventListener('change', onChange);
		return () => query.removeEventListener('change', onChange);
	}, []);

	return reduced;
};
