import { useRef } from 'react';

// The Lattice mark: 8 ellipses (rx 250, ry 95) rotated in even 22.5° steps around a
// shared centre. Geometry matches the brand SVG exactly, so the drawn mark is the
// real logo rather than an approximation of it.

const ROTATIONS = [0, 22.5, 45, 67.5, 90, 112.5, 135, 157.5];
const CENTER = 350;
const RX = 250;
const RY = 95;
const STROKE_WIDTH = 15.635;

interface RosetteProps {
	/** Rendered size, square. */
	size?: number | string;
	/** Draw the strokes on mount; false renders the finished mark. */
	animate?: boolean;
	title?: string;
}

const Rosette = ({ size = '3rem', animate = true, title = 'Lattice' }: RosetteProps) => {
	const groupRef = useRef<SVGGElement>(null);

	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="60 60 580 580"
			style={{ width: size, height: size, overflow: 'visible', flex: '0 0 auto' }}
			role="img"
			aria-label={title}
		>
			<g ref={groupRef}>
				{ROTATIONS.map((degrees, index) => (
					<ellipse
						key={degrees}
						className={animate ? 'rosette-stroke' : undefined}
						cx={CENTER}
						cy={CENTER}
						rx={RX}
						ry={RY}
						pathLength={1}
						transform={`rotate(${degrees} ${CENTER} ${CENTER})`}
						fill="none"
						stroke={animate ? undefined : 'var(--mark)'}
						strokeWidth={STROKE_WIDTH}
						strokeLinecap="round"
						style={animate ? { animationDelay: `${index * 0.09}s` } : undefined}
					/>
				))}
			</g>
		</svg>
	);
};

export default Rosette;
