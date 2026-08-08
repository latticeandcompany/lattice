import { Component, type ErrorInfo, type ReactNode } from 'react';

// A class, which the arrow-function rule allows only when the framework requires it —
// and React has no hook equivalent for catching a render error.
//
// Without this, anything thrown during render unmounts the whole tree and leaves an
// empty window, which says nothing about what went wrong. That is exactly what a
// black screen is, and it is the least debuggable failure a UI can have.

interface Props {
	children: ReactNode;
}

interface State {
	error: Error | null;
	stack: string | null;
}

class ErrorBoundary extends Component<Props, State> {
	state: State = { error: null, stack: null };

	static getDerivedStateFromError(error: Error): Partial<State> {
		return { error };
	}

	componentDidCatch(error: Error, info: ErrorInfo) {
		// The devtools console is the only other place this would surface, and a
		// packaged build has no devtools.
		console.error('Lattice: a render failed', error, info.componentStack);
		this.setState({ stack: info.componentStack ?? null });
	}

	render() {
		const { error, stack } = this.state;
		if (!error) return this.props.children;

		return (
			<div className="app-main__scroll">
				<div className="app-main__inner" style={{ maxWidth: '48rem' }}>
					<div className="notice notice--bad mb-3">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div>
							<strong className="d-block mb-1">Something in the window failed to draw.</strong>
							<span className="selectable">{error.message}</span>
						</div>
					</div>

					{stack && (
						<div className="card code-card">
							<div className="card-header code-card__bar">
								<span className="code-card__dot" />
								<span className="code-card__dot" />
								<span className="code-card__dot" />
								<span className="code-card__name">component stack</span>
							</div>
							<div className="card-body p-0">
								<pre className="code-card__body code-card__body--log mb-0 selectable">
									{stack.trim()}
								</pre>
							</div>
						</div>
					)}

					<button
						type="button"
						className="btn btn-primary mt-3 px-4"
						onClick={() => this.setState({ error: null, stack: null })}
					>
						Try again
					</button>
				</div>
			</div>
		);
	}
}

export default ErrorBoundary;
