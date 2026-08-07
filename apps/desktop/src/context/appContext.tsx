// The low-frequency state: which project is open, its config text, which view is
// showing. These change on a user action, so a plain reducer in context is right —
// a re-render of the tree when someone opens a project is not a problem worth
// solving with machinery.
//
// Run state is deliberately not here. It arrives faster than a frame and lives in
// `runStore`, which notifies per task.

import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useReducer,
	type ReactNode,
} from 'react';

import * as api from '../lib/api.ts';
import type {
	AppInfo,
	Catalog,
	ConfigDiagnostic,
	ProjectSnapshot,
	ProjectView,
	RecentProject,
} from '../lib/types.ts';

export type View = 'tasks' | 'graph' | 'config' | 'setup';

interface State {
	info: AppInfo | null;
	catalog: Catalog | null;
	project: ProjectView | null;
	configText: string | null;
	diagnostics: ConfigDiagnostic[];
	recents: RecentProject[];
	view: View;
	/** Set while a command is in flight, so the shell can say so. */
	busy: boolean;
	error: string | null;
	/** A directory the user picked that has no config yet. */
	pendingRoot: string | null;
}

type Action =
	| { type: 'info'; info: AppInfo }
	| { type: 'catalog'; catalog: Catalog }
	| { type: 'snapshot'; snapshot: ProjectSnapshot; root?: string }
	| { type: 'recents'; recents: RecentProject[] }
	| { type: 'view'; view: View }
	| { type: 'busy'; busy: boolean }
	| { type: 'error'; error: string | null }
	| { type: 'configText'; text: string }
	| { type: 'diagnostics'; diagnostics: ConfigDiagnostic[] }
	| { type: 'closed' };

const initial: State = {
	info: null,
	catalog: null,
	project: null,
	configText: null,
	diagnostics: [],
	recents: [],
	view: 'tasks',
	busy: false,
	error: null,
	pendingRoot: null,
};

const reduce = (state: State, action: Action): State => {
	switch (action.type) {
		case 'info':
			return { ...state, info: action.info };
		case 'catalog':
			return { ...state, catalog: action.catalog };
		case 'snapshot': {
			const { snapshot } = action;
			return {
				...state,
				project: snapshot.project,
				configText: snapshot.configText,
				diagnostics: snapshot.diagnostics,
				// A directory with no config is the wizard's starting point, not an error.
				pendingRoot: snapshot.project ? null : (action.root ?? state.pendingRoot),
				view: snapshot.project ? (state.view === 'setup' ? 'tasks' : state.view) : 'setup',
				error: null,
			};
		}
		case 'recents':
			return { ...state, recents: action.recents };
		case 'view':
			return { ...state, view: action.view };
		case 'busy':
			return { ...state, busy: action.busy };
		case 'error':
			return { ...state, error: action.error };
		case 'configText':
			return { ...state, configText: action.text };
		case 'diagnostics':
			return { ...state, diagnostics: action.diagnostics };
		case 'closed':
			return { ...initial, info: state.info, catalog: state.catalog, recents: state.recents };
		default:
			return state;
	}
};

interface AppValue extends State {
	setView: (view: View) => void;
	openProject: (path: string) => Promise<void>;
	pickAndOpen: () => Promise<void>;
	reload: () => Promise<void>;
	close: () => Promise<void>;
	forget: (root: string) => Promise<void>;
	setConfigText: (text: string) => void;
	saveConfig: (text: string) => Promise<void>;
	dismissError: () => void;
}

const AppContext = createContext<AppValue | null>(null);

export const AppProvider = ({ children }: { children: ReactNode }) => {
	const [state, dispatch] = useReducer(reduce, initial);

	const guard = useCallback(async (work: () => Promise<void>) => {
		dispatch({ type: 'busy', busy: true });
		try {
			await work();
		} catch (error) {
			dispatch({ type: 'error', error: error instanceof Error ? error.message : String(error) });
		} finally {
			dispatch({ type: 'busy', busy: false });
		}
	}, []);

	const refreshRecents = useCallback(async () => {
		const recents = await api.projectRecent();
		dispatch({ type: 'recents', recents });
	}, []);

	const openProject = useCallback(
		async (path: string) => {
			await guard(async () => {
				const snapshot = await api.projectOpen(path);
				dispatch({ type: 'snapshot', snapshot, root: path });
				await refreshRecents();
			});
		},
		[guard, refreshRecents],
	);

	const pickAndOpen = useCallback(async () => {
		const picked = await api.pickDirectory();
		if (picked) await openProject(picked);
	}, [openProject]);

	const reload = useCallback(async () => {
		await guard(async () => {
			const snapshot = await api.projectReload();
			dispatch({ type: 'snapshot', snapshot });
		});
	}, [guard]);

	const close = useCallback(async () => {
		await guard(async () => {
			await api.projectClose();
			dispatch({ type: 'closed' });
		});
	}, [guard]);

	const forget = useCallback(
		async (root: string) => {
			await guard(async () => {
				await api.projectForget(root);
				await refreshRecents();
			});
		},
		[guard, refreshRecents],
	);

	const saveConfig = useCallback(
		async (text: string) => {
			await guard(async () => {
				const snapshot = await api.configSave(text);
				dispatch({ type: 'snapshot', snapshot });
			});
		},
		[guard],
	);

	useEffect(() => {
		void (async () => {
			try {
				const [info, catalog, recents] = await Promise.all([
					api.appInfo(),
					api.catalog(),
					api.projectRecent(),
				]);
				dispatch({ type: 'info', info });
				dispatch({ type: 'catalog', catalog });
				dispatch({ type: 'recents', recents });
				// The platform decides whether our own chrome has to leave room for the
				// traffic lights.
				document.documentElement.setAttribute('data-platform', info.platform);
			} catch (error) {
				dispatch({
					type: 'error',
					error: error instanceof Error ? error.message : String(error),
				});
			}
		})();
	}, []);

	const value = useMemo<AppValue>(
		() => ({
			...state,
			setView: (view) => dispatch({ type: 'view', view }),
			openProject,
			pickAndOpen,
			reload,
			close,
			forget,
			setConfigText: (text) => dispatch({ type: 'configText', text }),
			saveConfig,
			dismissError: () => dispatch({ type: 'error', error: null }),
		}),
		[state, openProject, pickAndOpen, reload, close, forget, saveConfig],
	);

	return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
};

export const useApp = (): AppValue => {
	const value = useContext(AppContext);
	if (!value) throw new Error('useApp must be used inside AppProvider');
	return value;
};
