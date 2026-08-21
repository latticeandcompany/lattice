import { createRoot } from 'react-dom/client';

// Load order matches the website: Tailwind's layers, then Bootstrap's compiled
// output, then our globals, then the sheets only this app has.
import './styles/tailwind.css';
import './styles/bootstrap.scss';
import './globals.scss';
import './styles/shell.scss';
import './styles/codeCard.scss';
import './styles/steps.scss';
import './styles/tasks.scss';
import './styles/graph.scss';

import App from './app';

const host = document.getElementById('root');
if (!host) throw new Error('index.html is missing #root');

createRoot(host).render(<App />);
