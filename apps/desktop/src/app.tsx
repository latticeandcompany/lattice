import { AppProvider } from './context/appContext.tsx';
import AppShell from './components/appShell.tsx';

const App = () => (
	<AppProvider>
		<AppShell />
	</AppProvider>
);

export default App;
