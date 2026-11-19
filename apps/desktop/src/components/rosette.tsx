import { STROKES, type RosetteProgress } from '../lib/progress.ts';

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
	/** Weave the mark as a run settles: drawn strokes take the run's colour. */
	progress?: RosetteProgress;
	title?: string;
}

const Rosette = ({ size = '3rem', animate = true, progress, title = 'Lattice' }: RosetteProps) => {
	const className = progress ? `rosette-progress rosette-progress--${progress.tone}` : undefined;

	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="60 60 580 580"
			className={className}
			style={{ width: size, height: size, overflow: 'visible', flex: '0 0 auto' }}
			role="img"
			aria-label={title}
		>
			{ROTATIONS.map((degrees, index) => {
				const strand = progress
					? `rosette-strand${index < Math.min(progress.drawn, STROKES) ? ' rosette-strand--drawn' : ''}`
					: animate
						? 'rosette-stroke'
						: undefined;
				return (
					<ellipse
						key={degrees}
						className={strand}
						cx={CENTER}
						cy={CENTER}
						rx={RX}
						ry={RY}
						pathLength={1}
						transform={`rotate(${degrees} ${CENTER} ${CENTER})`}
						fill="none"
						stroke={animate || progress ? undefined : 'var(--mark)'}
						strokeWidth={STROKE_WIDTH * (progress ? 1.6 : 1)}
						strokeLinecap="round"
						style={animate && !progress ? { animationDelay: `${index * 0.09}s` } : undefined}
					/>
				);
			})}
		</svg>
	);
};

export default Rosette;
