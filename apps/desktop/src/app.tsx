import ErrorBoundary from './components/errorBoundary.tsx';
import { AppProvider } from './context/appContext.tsx';
import AppShell from './components/appShell.tsx';

const App = () => (
	<ErrorBoundary>
		<AppProvider>
			<AppShell />
		</AppProvider>
	</ErrorBoundary>
);

export default App;
