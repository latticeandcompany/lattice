import { useEffect, useRef } from 'react';

// The Lattice mark: 8 ellipses (rx 250, ry 95) rotated in even 22.5° steps around
// a shared center, forming the woven-sphere rosette. Geometry mirrors
// marketing/lattice_icon_black.svg exactly so the drawn mark is the real logo.

const ROTATIONS = [0, 22.5, 45, 67.5, 90, 112.5, 135, 157.5];
const CENTER = 350;
const RX = 250;
const RY = 95;
const STROKE_WIDTH = 15.635;

interface RosetteProps {
	/** Rendered size in px (square). */
	size?: number | string;
	/** Draw the strokes on mount; false renders the finished mark. */
	animate?: boolean;
	/** Force a stroke color (used on the always-dark pattern band). Defaults to the theme mark color. */
	stroke?: string;
	className?: string;
	title?: string;
}

const Rosette = ({ size = '3rem', animate = true, stroke, className, title = 'Lattice' }: RosetteProps) => {
	const groupRef = useRef<SVGGElement>(null);

	// Replay the draw after an Astro view-transition swap so returning to a page
	// re-animates the mark rather than showing it statically.
	useEffect(() => {
		if (!animate) return;
		const replay = () => {
			const group = groupRef.current;
			if (!group) return;
			group.querySelectorAll<SVGEllipseElement>('.rosette-stroke').forEach((el) => {
				el.style.animation = 'none';
				void el.getBoundingClientRect();
				el.style.animation = '';
			});
		};
		document.addEventListener('astro:after-swap', replay);
		return () => document.removeEventListener('astro:after-swap', replay);
	}, [animate]);

	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="60 60 580 580"
			style={{ width: size, height: size, overflow: 'visible' }}
			role="img"
			aria-label={title}
		>
			<g ref={groupRef}>
				{ROTATIONS.map((deg, i) => (
					<ellipse
						key={deg}
						className={animate ? 'rosette-stroke' : undefined}
						cx={CENTER}
						cy={CENTER}
						rx={RX}
						ry={RY}
						pathLength={1}
						transform={`rotate(${deg} ${CENTER} ${CENTER})`}
						fill="none"
						stroke={stroke ?? (animate ? undefined : 'var(--mark)')}
						strokeWidth={STROKE_WIDTH}
						strokeLinecap="round"
						style={animate ? { animationDelay: `${i * 0.09}s`, stroke } : undefined}
					/>
				))}
			</g>
		</svg>
	);
};

export default Rosette;
