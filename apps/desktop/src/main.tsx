import { createRoot } from 'react-dom/client';

// Bootstrap's compiled output first, then the base layer, then the sheets for each
// part of the window.
import './styles/bootstrap.scss';
import './globals.scss';
import './styles/shell.scss';
import './styles/controls.scss';
import './styles/runSheet.scss';
import './styles/graph.scss';
import './styles/panels.scss';
import './styles/config.scss';
import './styles/wizard.scss';

import App from './app';

const host = document.getElementById('root');
if (!host) throw new Error('index.html is missing #root');

createRoot(host).render(<App />);
