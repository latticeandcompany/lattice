// The only module that talks to Tauri.
//
// Everything is wrapped so the rest of the app never imports `@tauri-apps/*`, and
// so a call that fails arrives as an Error with the backend's message rather than a
// bare rejected string.

import { Channel, invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';

import type {
	AppInfo,
	Catalog,
	ConfigDiagnostic,
	GraphDump,
	ProjectSnapshot,
	ProjectView,
	RecentProject,
	RunMessage,
	RunOptions,
	RunOutcome,
	OutputLine,
	ScanResult,
	WorkspaceCandidate,
	EnginePin,
} from './types.ts';

/** True inside the app, false under a plain `vite dev`. */
export const inTauri = (): boolean => '__TAURI_INTERNALS__' in window;

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	try {
		return await invoke<T>(command, args);
	} catch (error) {
		// A command rejects with the string the backend formatted, which already
		// carries the whole anyhow context chain. Wrapping it keeps callers on one
		// error type.
		throw new Error(typeof error === 'string' ? error : String(error));
	}
};

export const appInfo = () => call<AppInfo>('app_info');
export const catalog = () => call<Catalog>('catalog');

export const pickDirectory = async (): Promise<string | null> => {
	// The dialog plugin has a frontend API, so this needs no command of its own.
	const picked = await open({ directory: true, multiple: false, title: 'Open a Lattice repo' });
	return typeof picked === 'string' ? picked : null;
};

export const projectOpen = (path: string) => call<ProjectSnapshot>('project_open', { path });
export const projectCurrent = () => call<ProjectView | null>('project_current');
export const projectReload = () => call<ProjectSnapshot>('project_reload');
export const projectClose = () => call<void>('project_close');
export const projectRecent = () => call<RecentProject[]>('project_recent');
export const projectForget = (root: string) => call<void>('project_forget', { root });
export const projectFindRoot = (start: string) => call<string | null>('project_find_root', { start });

export const configValidate = (text: string) => call<ConfigDiagnostic[]>('config_validate', { text });
export const configSave = (text: string) => call<ProjectSnapshot>('config_save', { text });
export const configSchema = () => call<unknown>('config_schema');

export interface InitSelection {
	candidates: WorkspaceCandidate[];
	pins: EnginePin[];
}

export const configScan = (path: string) => call<ScanResult>('config_scan', { path });
export const configPreview = (selection: InitSelection) => call<string>('config_preview', { selection });
export const configInit = (path: string, selection: InitSelection, force: boolean) =>
	call<ProjectSnapshot>('config_init', { path, selection, force });

export interface PlanRequest {
	tasks: string[];
	filter?: string;
	sequentially: boolean;
}

export const graphDump = (request: PlanRequest) => call<GraphDump>('graph_dump', { request });

/**
 * Start a run and resolve when it ends.
 *
 * A Channel rather than an event: Tauri's own docs say the event system is not
 * built for high throughput, and name subprocess output as what channels are for.
 */
export const runStart = (
	options: RunOptions,
	onMessage: (message: RunMessage) => void,
): Promise<RunOutcome> => {
	const channel = new Channel<RunMessage>();
	channel.onmessage = onMessage;
	// The backend flattens PlanRequest into RunRequest, so the wire shape is one
	// object rather than a nested plan.
	return call<RunOutcome>('run_start', { request: options, onMessage: channel });
};

export const runStop = () => call<boolean>('run_stop');
export const runActive = () => call<string | null>('run_active');
export const runLog = (workspace: string, task: string) =>
	call<OutputLine[]>('run_log', { workspace, task });

/**
 * Push the theme at the native window so the traffic lights, scrollbars, and
 * native menus follow it.
 *
 * Order matters: setting an explicit theme makes the *webview* report that theme
 * through prefers-color-scheme, so following the system means handing the window
 * null and letting matchMedia read the OS rather than our own override.
 */
export const setWindowTheme = async (theme: 'light' | 'dark' | null): Promise<void> => {
	if (!inTauri()) return;
	try {
		await getCurrentWindow().setTheme(theme);
	} catch {
		// Not fatal: the CSS attribute is what actually paints the app.
	}
};
