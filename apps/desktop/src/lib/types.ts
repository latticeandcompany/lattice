// The wire shapes the Rust backend sends and accepts.
//
// Hand-written rather than generated: there are about twenty of them, and a
// generator would be a dependency plus a build step to keep them honest. What
// keeps them honest instead is a test per variant in `crates/lattice-events` and
// `crates/dagger` asserting the JSON, so a Rust rename fails a test whose expected
// shape a reviewer has to edit -- which puts this file in the same diff.

export interface AppInfo {
	latticeVersion: string;
	platform: 'macos' | 'windows' | 'linux';
	keyComponents: string[];
}

export interface RecentProject {
	root: string;
	name: string;
	openedAt: number;
}

// ---------- config, mirroring crates/lattice-config ----------

export type EngineSpec = string | EngineSpecObject;

export interface EngineSpecObject {
	version?: string;
	versionCmd?: string;
	installCmd?: string;
	bin?: string;
}

export interface WorkspaceConfig {
	name: string;
	path: string;
	auto?: boolean;
	engines?: Record<string, EngineSpec>;
	dependsOn?: string[];
	scripts?: Record<string, string>;
}

export interface PipelineTask {
	dependsOn?: string[];
	inputs?: string[];
	outputs?: string[];
	ignore?: string[];
	env?: string[];
	persistent?: boolean;
	cache?: boolean;
	/** A human duration: "90s", "5m", "1h", or a bare number of seconds. */
	timeout?: string | number;
}

export interface Settings {
	maxCacheSize?: string;
	cacheDir?: string;
	loquacious?: boolean;
	versionCheck?: boolean;
}

export interface LatticeConfig {
	$schema?: string;
	latticeVersion?: string;
	workspaces?: WorkspaceConfig[];
	engines?: Record<string, EngineSpec>;
	globalDependencies?: string[];
	globalEnv?: string[];
	tasks?: Record<string, PipelineTask>;
	settings?: Settings;
}

// ---------- a resolved project ----------

export type DriverRole = 'runtime' | 'packageManager' | 'buildTool' | 'taskRunner';

export type Evidence =
	| { kind: 'declaration' }
	| { kind: 'nativeFile'; file: string }
	| { kind: 'lockfile'; file: string };

export interface DriverView {
	tool: string;
	role: DriverRole;
	/** The ecosystem slug a language mark is keyed off. Null for `just` and `task`. */
	language: string | null;
	via: Evidence;
}

export interface EngineView {
	name: string;
	version: string | null;
	versionCmd: string | null;
	installCmd: string | null;
	bin: string | null;
	wellKnown: boolean;
}

export interface WorkspaceTaskView {
	task: string;
	command: string;
	persistent: boolean;
	cacheable: boolean;
}

export interface WorkspaceView {
	name: string;
	/** Repo-relative, forward-slashed on every platform. */
	path: string;
	auto: boolean;
	dependsOn: string[];
	driver: DriverView | null;
	engines: EngineView[];
	tasks: WorkspaceTaskView[];
}

export interface TaskDefView {
	name: string;
	dependsOn: string[];
	inputs: string[];
	outputs: string[];
	ignore: string[];
	env: string[];
	persistent: boolean;
	cache: boolean;
	timeout: string | null;
}

export interface SettingsView {
	maxCacheSize: string | null;
	cacheDir: string;
	loquacious: boolean;
	versionCheck: boolean;
}

export interface ProjectView {
	root: string;
	name: string;
	latticeVersion: string | null;
	tasks: TaskDefView[];
	workspaces: WorkspaceView[];
	engines: EngineView[];
	globalDependencies: string[];
	globalEnv: string[];
	settings: SettingsView;
}

/** What the app holds after opening a directory. */
export interface ProjectSnapshot {
	/** Null when the directory has no lattice.json — the wizard's entry point. */
	project: ProjectView | null;
	/** The file's bytes as text. The config editor's source of truth. */
	configText: string | null;
	diagnostics: ConfigDiagnostic[];
}

export interface ConfigDiagnostic {
	severity: 'error' | 'warning';
	message: string;
	/** 1-based, when the failure came with a position. */
	line: number | null;
	column: number | null;
}

// ---------- scan and catalog ----------

export interface WorkspaceCandidate {
	name: string;
	path: string;
	marker: string;
	driver: string | null;
	defaultSelected: boolean;
}

export interface EnginePin {
	engine: string;
	version: string;
	source: string;
}

export interface ScanResult {
	root: string;
	/** True when the directory already holds a lattice.json. */
	alreadyInitialized: boolean;
	candidates: WorkspaceCandidate[];
	pins: EnginePin[];
}

export interface DriverCatalogEntry {
	tool: string;
	roles: DriverRole[];
	language: string | null;
	fingerprint: string[];
	versionCmd: string;
}

export interface EngineCatalogEntry {
	name: string;
	versionCmd: string | null;
}

export interface Catalog {
	drivers: DriverCatalogEntry[];
	engines: EngineCatalogEntry[];
	lockfiles: string[];
	manifests: string[];
	keyComponents: string[];
}

// ---------- graph ----------

export interface GraphNode {
	/** `workspace:task` — the same label a run reports. */
	id: string;
	workspace: string;
	task: string;
	command: string;
	persistent: boolean;
	/** In the graph only because something selected depends on it. */
	pulledIn: boolean;
}

export interface GraphEdge {
	/** Must finish before `to` starts. */
	from: string;
	to: string;
	crossWorkspace: boolean;
}

/** `nodes` is in topological order, so a layered drawing can read it directly. */
export interface GraphDump {
	nodes: GraphNode[];
	edges: GraphEdge[];
}

// ---------- running ----------

export interface RunOptions {
	tasks: string[];
	/** Substring matched against workspace names, as `--filter` does. */
	filter?: string;
	sequentially: boolean;
	concurrency?: number;
	keepGoing: boolean;
	/** Skip cache lookups. Both Force and Ignore cache set this. */
	noCache: boolean;
	/** Also re-run and refresh the entry. Force sets this; Ignore cache does not. */
	force: boolean;
}

export type CacheMiss =
	| { kind: 'firstRun' }
	| { kind: 'entryEvicted' }
	| { kind: 'changed'; components: string[] };

export type TaskEvent =
	| { type: 'started'; workspace: string; task: string }
	| { type: 'cacheHit'; workspace: string; task: string; key: string }
	| { type: 'cacheMiss'; workspace: string; task: string; miss: CacheMiss }
	| {
			type: 'output';
			workspace: string;
			task: string;
			line: string;
			stderr: boolean;
			persistent: boolean;
	  }
	| { type: 'finished'; workspace: string; task: string; durationMs: number }
	| {
			type: 'failed';
			workspace: string;
			task: string;
			/** Null when a signal ended it, or when the task failed before it ran. */
			code: number | null;
			/** Null when the task failed before its command ran, so there was no run to time. */
			durationMs: number | null;
	  }
	| {
			type: 'persistentExited';
			workspace: string;
			task: string;
			/** Null when a signal ended it. */
			code: number | null;
			durationMs: number;
	  }
	| { type: 'skipped'; workspace: string; task: string; reason: string };

export interface OutputLine {
	stderr: boolean;
	line: string;
}

export interface RunResult {
	total: number;
	cached: number;
	failed: number;
	elapsedMs: number;
}

export type RunOutcome =
	| { status: 'completed'; result: RunResult }
	| { status: 'failed'; result: RunResult }
	| { status: 'interrupted'; result: RunResult }
	| { status: 'nothing' };

/**
 * What arrives on the run channel.
 *
 * `outputBatch` exists because a compiler emits lines faster than an IPC message
 * per line can carry them: the backend coalesces them on a timer.
 */
export type RunMessage =
	| { kind: 'runStarted'; runId: string; graph: GraphDump }
	| { kind: 'event'; event: TaskEvent }
	| { kind: 'outputBatch'; lines: (OutputLine & { workspace: string; task: string })[] }
	| { kind: 'failureOutput'; workspace: string; task: string; lines: OutputLine[] }
	| { kind: 'note'; workspace: string | null; task: string | null; message: string }
	| { kind: 'warn'; workspace: string | null; task: string | null; message: string }
	| { kind: 'summary'; result: RunResult }
	| { kind: 'finished'; runId: string; outcome: RunOutcome };
