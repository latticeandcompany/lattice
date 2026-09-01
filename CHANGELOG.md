# Changelog

Notable changes, newest first. Versions follow semver: a major bump means a
breaking change to the `lattice.json` schema or the CLI surface.

The running Lattice version is one of the inputs hashed into every task's cache
key, so a version bump is a full cache miss and the first run after an upgrade
re-runs everything.

## Unreleased

<!--
Add an entry as a `###` heading: what changed, stated plainly, then " — " and the
date as YYYY-MM-DD. Under the heading, write prose or bullets, one fact per
bullet. Where a reader needs it, say what the previous behavior was. Do not use
`Added`/`Changed`/`Fixed` buckets, bold lead-ins, or marketing.
-->

### A version reference that is really a range no longer counts — 2026-08-31

- `scripts/sync-version.sh` rewrote the outgoing version literally wherever it
  appeared in the files `scripts/version-doc-files.txt` lists. At 1.0.0 that
  reaches text it has no business touching: three pages quote the engine error's
  `{ "version": ">=1.0.0" }`, and a sample of npm output reads `web@1.0.0`. The
  next bump would have rewritten all four into error messages the code does not
  print. A version that follows a range operator or an `@` is now left alone
- `scripts/check-versions.sh` makes the same exception, so the two still agree on
  what counts as a reference. Without it, every page holding a `>=1.0.0` example
  would have failed the check that a page naming the current version is on the
  list
- The check now scans `apps/web/src/pages/*.astro` as well as the docs and the
  skill, and `get-started.astro` joins the list. Its terminal transcripts are
  captured output, and nothing was watching them: the page said `1.0.0-beta-2`,
  two releases behind, while the check passed
- The transcripts on that page are re-captured from 1.0.0, and it teaches `-v`
  rather than `-l`. `-l` is a hidden alias for `--verbose`, so the site was the
  one place advertising a flag `--help` does not list

### Lattice 1.0 — 2026-08-31

- The first stable release. The `lattice.json` schema and the CLI surface are
  under semver from here: a breaking change to either takes a major bump
- The GitHub release is no longer marked a pre-release and the npm packages
  publish under `latest` rather than `next`, so a bare `install.sh`, a bare
  `npm install --save-dev @latticeandcompany/lattice`, and `lattice upgrade`
  with no version all resolve to it. The `@next` tag is no longer the one to ask
  for, and the docs no longer tell you to
- Nothing about the CLI, the config schema, or the cache key changed from
  `1.0.0-beta-3`. The running version is hashed into every task's key, so the
  first run after upgrading still re-runs everything

### Ctrl-C ends a run holding persistent tasks — 2026-08-26

- A run whose persistent task left a process holding the task's output open
  outside the process group Lattice signalled would never exit. The run's final
  drain waited for its message channel to close, the channel closes when the
  last output streamer sees EOF, and that EOF never came. `tauri dev` starting
  its `beforeDevCommand` in a process group of its own is enough to trigger it,
  so `lattice run dev` on a Tauri workspace hung on every interrupt
- The drain now gives up after 500ms and warns that a process was left running,
  rather than waiting for a pipe no signal can close. Trailing output from a
  task can be cut short in that case; previously there was no output at all,
  because the run never reached its summary
- A second Ctrl-C now exits immediately with 130. Watching for an interrupt
  replaces the default action for both SIGINT and SIGTERM for the rest of the
  process's life, so once the first interrupt had been consumed no signal could
  end the run and `kill -9` was the only way out — which made any hang during
  teardown look like Ctrl-C being ignored

### Every version reference updates from one list — 2026-08-26

- `scripts/version-doc-files.txt` is a new list of the files whose version
  references have to track the release: the README, seven docs pages, and the
  three agent-skill files. `scripts/sync-version.sh` rewrites exactly that list
  and `scripts/check-versions.sh` asserts it, so the two cannot disagree about
  what needs updating
- The sync replaces the outgoing version literally rather than matching a
  pattern. Several pages quote versions that must never move: `upgrading.md`
  walks through 0.1.0 to 0.2.0 and includes a fictional `"latticeVersion":
  "0.2.0"`, `errors.md` pins a fictional 0.4.0 to show a mismatch, and the README
  says `lattice upgrade 0.2.0`. Anchoring on the `latticeVersion` key or on a
  semver shape would have rewritten all of them
- Previously only the README's sample `latticeVersion` was written, by a
  key-anchored `sed`, and nothing asserted it. That is how it came to sit a
  release behind: the docs, the skill, and the README all still said
  `1.0.0-beta-2`. They now say `1.0.0-beta-3`
- The check runs both directions. A listed file that does not name the current
  version fails and names the script to run. A docs or skills page that names the
  current version and is not on the list also fails, so a page that grows a
  version sample is caught when it is written rather than at the next bump.
  Changelogs are exempt: an entry names the version that shipped it
- CI already runs `check-versions.sh`, so both assertions gate every pull request
  and every tag

### The docs stop explaining how the docs get made — 2026-08-26

- The changelog page opened by naming `CHANGELOG.md` as the source of record and
  describing which entries it reproduced. The CLI reference said that where it
  and `--help` disagreed, the page was a docs bug. The glossary said it was
  alphabetical. All deleted, along with a line of docs policy in the
  architecture page
- The README lost `## Development` and `## Architecture`. Both repeated content
  that `CONTRIBUTING.md` and the architecture page carry in fuller form, and the
  crate tree had to be hand-synced against the workspace
- The README said Lattice's drivers cover twelve languages "and four
  language-agnostic task runners". Two are: `just` and `task`. `turbo` and `nx`
  belong to the Node ecosystem

### Lattice installs from npm — 2026-08-26

- New package: `@latticeandcompany/lattice`. `npm install --save-dev
  @latticeandcompany/lattice` puts `lattice` on a repo's package scripts and
  `npx lattice` on the command line. `pnpm`, `yarn`, and `bun` take the same
  package name. Pre-1.0 releases publish under the `next` dist-tag, so a bare
  install keeps resolving to the last stable one
- The binary is not in that package. Six sibling packages carry one build each,
  one per published target, and the wrapper depends on all six as
  `optionalDependencies`. Each declares the `os`, `cpu`, and `libc` it is for, so
  a package manager unpacks only the one that matches. The install downloads
  nothing beyond those packages and runs no `postinstall` script, so it works
  offline, from a lockfile, behind a proxy, and under `--ignore-scripts`
- The binaries are the ones the release already publishes.
  `scripts/publish-npm.sh` downloads a released tag's archives, checks them
  against the checksums file published beside them, and repackages those exact
  bytes
- An npm install does not follow a `latticeVersion` pin, the one way it differs
  from `install.sh`. A binary under `.lattice/bin` switches to the pinned
  version. A binary in `node_modules` cannot, because the lockfile has already
  chosen the version. Lattice runs the installed version and prints the mismatch
  on stderr. `settings.versionCheck`, `--no-version-check`, and
  `LATTICE_NO_VERSION_CHECK` silence that warning, and they silence the binary's
  own version check the same way
- The package exports `binaryPath()`, the resolved path to the binary, so another
  tool can spawn Lattice directly instead of running `npx`. It exports `version`
  beside it. `import` and `require()` both reach the two
- Linux x86_64 gets both a glibc and a musl package. Package managers honor npm's
  `libc` field inconsistently, so the wrapper also chooses between the two at
  runtime. Windows on ARM gets the x86_64 build, which is the fallback the
  installers already make. Lattice publishes no build for Linux, aarch64 (musl),
  and says so rather than guessing
- `scripts/sync-version.sh` now also writes `apps/desktop/package.json` and its
  lockfile. `check-versions.sh` has always asserted that version, but nothing
  wrote it, so a bump left the tree failing its own check

### A cached run reports the time it saved, and `lattice stats` adds it up — 2026-08-23

- The run summary reports the task time the run's cache hits skipped:
  `lattice: 10 tasks, 8 cached, 0 failed, 4.20s, 2m 51s saved`, and
  `❖  10 tasks · 8 cached · 0 failed  4.20s · 2m 51s saved` in the live display.
  It appears on any run with hits, not only a fully-cached one, and a run whose
  hits saved nothing measurable does not mention it
- The figure is task time, not wall clock. Every cache entry records how long the
  run that wrote it took, and a hit adds that recorded duration back. Four cached
  one-minute tasks that would have run in parallel therefore read as `4m 00s
  saved`, where the wall clock would only have shown about a minute
- Under a minute the saved figure is written like the elapsed time beside it,
  `4.27s`. Past a minute it reads `4m 07s`, and past an hour `14h 22m`
- New command: `lattice stats`. It reports the task time this repo's cache has
  saved, the runs and hits behind that number, how much room the cache takes, and
  the last seven days. A repo that has never run says so and exits 0
- The record behind it is a ledger at `.lattice/cache/stats.jsonl`, one JSON
  object appended per run. It is per-machine and already ignored by the
  `.lattice/cache/` line `lattice init` writes. It lives with the cache so that it
  follows a relocated `cacheDir`, which also means clearing the cache clears the
  history. `lattice prune` and the leftover sweep leave it alone, and a line that
  no longer parses costs that run's numbers and nothing else
- A run appends nothing when it was told not to use the cache, or when it
  scheduled no task at all
- The banner under a fully-cached run reads `❖❖❖ FULL POWER`. It was
  `❖❖❖ FULL CACHE`, which is the phrasing of a disk running out of room — the
  opposite of what a run that skipped all its work just did. The raw stream's
  marker line changed with it, from `lattice: full cache, nothing to run` to
  `lattice: full power, nothing to run`. A CI job grepping for the old string has
  to be updated. What triggers it has not changed: at least one task scheduled,
  no failures, and a hit for every one
- The `cacheHit` event carries `savedMs`, the recorded task time that hit
  skipped, and a run's result carries the run's total as `savedMs`. The desktop
  app's run bar reports the same saved figure

### A failure names its exit code, and the live display drops its trace lines — 2026-08-23

- The live display no longer prints per-task trace lines. The dim
  `ui:build: hash 5341be256174fcac` and `ui:build: cache miss: inputs changed`
  lines above a running task are gone. A hit already prints its abbreviated key
  on the task's own line, `● ui:build  cache hit [5341be25]`, and a miss shows up
  as the task running. The trace prints in the raw stream under `-v`, which is
  where it was always documented to live
- Notes about the run as a whole still print dim in the live display:
  provisioning a toolchain, and pruning the cache
- The live display's failed line carries the command's exit code and how long the
  task ran: `✗ ui:build  FAILED (code 3) 1.02s`
- The raw stream's failed line carries the same detail:
  `ui:build: FAILED (code 3) after 1.03s`. It was the bare word `FAILED`
- A line drops the detail it does not have. A task a signal killed, and a task
  stopped for overrunning its `timeout`, have no exit code, so the line reads
  `FAILED after 30.00s`. A task that failed before its command ever ran has
  neither, so the line is the bare word. Its cache key would not compute, or its
  shell would not spawn, and reporting `0.00s` for a task that never started
  would be a lie
- A failing task's captured output reads in arrival order. Both of a child's
  pipes now append to one buffer as each line is read, so a compiler's error
  lines stay next to the context they belong to. The whole of stdout used to
  print and then the whole of stderr, which left the error and its context pages
  apart. This is arrival order as Lattice reads it, so a task that dumps both
  streams in one burst can still print one stream before the other. Output that
  arrives spread over time is in the order it happened
- The failure block in the live display prints at normal brightness, with a
  blank line above its `✗ ui:build output` header. It was dim, which is the wrong
  treatment for the one thing on the screen worth reading
- The `failed` event carries `code` and `durationMs`, the two fields
  `persistentExited` already had. Both are `null` for a task that failed before
  its command ran, and a `durationMs` of `0` means a command that ran and failed
  inside a millisecond. The desktop app's failed task row reads
  `failed (code 101) 1.84s`

### A task's command is never invented, and a skipped task says so — 2026-08-23

- A workspace driven from a manifest now only ever runs a script that manifest
  declares. `npm`, `pnpm`, `yarn`, and `bun` read `scripts` in `package.json`,
  and `deno` reads `tasks` in `deno.json` or `deno.jsonc`. A requested task the
  manifest does not hold drops out of the graph and the run carries on. Lattice
  used to fabricate `npm run <task>` for it, and the fabricated command failed
  the build
- The check could not tell four situations apart, and answered all of them the
  same way: a driver that takes the task name on its command line, a manifest
  that could not be read, a manifest that would not parse, and a manifest with no
  script section. Each is now handled separately
- A workspace whose manifest declares a script map that does not hold the task
  being run is named in a warning, because a typo looks the same:
  `web declares scripts but no "build", so the task was skipped. Did you mean
  "biuld"?`. With no near miss to offer, the warning ends `Declare it in the
  workspace's manifest, or under "scripts" in lattice.json, if the task should run
  there`. Two or more such workspaces collapse into one line, `some tasks were
  skipped: ...`, and each keeps its own suggestion
- The noun after `declares` is the manifest's own word for the map, so a deno
  workspace says `tasks`. The warning appears once per run, on `--dry-run` as
  well as a real run, and covers only the tasks being run. `lattice run build`
  says nothing about a missing `lint`. `--filter` does not narrow the warning, so
  a manifest that should declare `build` and does not is reported even when this
  run does not select that workspace
- A manifest with no script map at all stays silent, and so does a workspace with
  `auto: false`. A package that declares nothing is a finished configuration
- A manifest Lattice cannot read is now reported rather than worked around, one
  warning per workspace: `web: package.json could not be parsed: <reason>, so
  every task it would have named was skipped`. The other two reasons are a file
  that could not be read and a `scripts` that is not an object
- Lattice strips `//` and `/* */` comments before it parses `deno.json` and
  `deno.jsonc`, so a commented Deno config resolves its tasks instead of counting
  as unparsable. `package.json` is still parsed as strict JSON, the way npm
  parses it

### A pinned toolchain no longer records a version it did not read — 2026-08-23

- An engine whose version cannot be read, or that has no `versionCmd` and no
  built-in rule, used to record `0.0.0`. That version was never installed, the
  constraint was never checked, and the fabricated number went into `pins.json`
  and into every cache key in the workspace
- With a `version` constraint it is now an error: ``engine 'alpes' has the
  version constraint '>=2' but no way to check what was installed. 'alpes' is not
  a well-known engine, so add a `versionCmd` to it``. When the version command
  ran and printed no number, the error is ``could not read a version from the
  output of `<cmd>` after install, so the constraint '<cons>' cannot be checked``
- With no constraint the recorded version is `unknown`, and the install hash is
  what identifies the toolchain. The cache key's toolchain component reads
  `<name>=unknown@<hash>` where it used to read `<name>=0.0.0@<hash>`, so those
  keys move once
- A staging directory is now stamped with the process id, so two `lattice setup`
  runs that provision one engine at the same time each get their own. They used
  to share one, clear it before use, and promote a tree assembled from both
  installs. Staging left behind by a run that was killed is deleted once it is 24
  hours old
- An engine's `bin` has to be relative and inside the toolchain install. It is
  checked when the config loads, and now also when a pin is read back out of
  `pins.json`, so a hand-edited pin cannot put a directory Lattice never
  provisioned in front of every command
- Failing to build the pinned `PATH` is an error rather than a silent fall back
  to the host's tool. All three places that build one report it: an engine's
  version or install command, `lattice setup`'s dependency installer, and
  spawning a task. The message is `the pinned toolchain cannot be put on PATH,
  because a directory in it contains a character PATH cannot hold: <dirs>`, and
  on a task it is reported as that task's failure. Lattice used to drop that
  directory in silence, so the task ran against whatever version of the tool the
  machine had while the run still reported a provisioned toolchain
- A workspace directory that resolves outside the repo through a symlink is
  refused: `workspace path 'app' resolves to <path>, which is outside the repo
  root. A workspace directory has to be inside the repo`. The directory a path
  resolves to is what bounds a task's inputs and outputs. A symlink that stays
  inside the repo is still a workspace
- `lattice init` can no longer propose two workspaces with the same name. The
  path-based fallback flattened `a/b-c` and `a-b/c` to the same string, and a
  config with two workspaces of one name does not load. The second now gets a
  `-2` suffix

### A run ends on the first Ctrl-C, and a cancelled run reports no failures — 2026-08-23

- `Ctrl-C` now ends a run that persistent tasks are holding open on the first
  press. It needed a second press before, because the wait started listening only
  after the graph had drained and so missed the signal that had already fired
- `SIGTERM`, which is what a CI runner sends when a job is cancelled, now ends the
  run. It previously hung until the runner force-killed it, so a cancelled job
  lost the rest of its timeout
- A task failure now ends a run a dev server is holding open, instead of waiting
  for a signal only a person was going to send
- A task whose children ignore the stop signal is escalated to a force kill when
  the grace period runs out. A `timeout` on such a task therefore reports up to
  five seconds late. It previously never reported at all, because the run went on
  waiting on pipes nothing would close
- A task's output survives a byte that is not valid UTF-8, rendered as U+FFFD.
  Everything after such a byte used to be dropped for the rest of that task, and
  the line explaining a failure usually comes after the noise that caused it
- A task stopped as the run shuts down is no longer reported as a failure. The
  events and the summary now agree: an interrupted run prints no `FAILED` line
  for a task the interrupt stopped, above a summary that already said `0 failed`.
  Read the exit code, `130`, rather than the summary

### `lattice setup` notices a lockfile above the workspace — 2026-08-23

- A lockfile hoisted above a workspace now invalidates its install marker. Only
  the workspace's own directory was checked before, so in the everyday npm, pnpm,
  or yarn workspace tree, where the only lockfile sits at the repo root,
  `lattice setup` printed `dependencies up to date` forever and the next build
  failed on a dependency that had never been installed. Only `--force` recovered
- The install marker moved out of the workspace directories. It is now one flat
  file per workspace under `.lattice/setup`, named for the workspace's path
  relative to the repo root with `/` written `%2F` and `%` written `%25`, so
  `apps/web` gets `.lattice/setup/apps%2Fweb.marker`, `apps-web` gets
  `.lattice/setup/apps-web.marker`, and the repo root as a workspace gets
  `.lattice/setup/.marker`. Encoding the separator rather than replacing it means
  two paths can never share a marker
- A marker at the old `<workspace>/.lattice-setup-marker` path is still honored,
  so upgrading mid-project reinstalls nothing. Lattice deletes that marker once an
  install succeeds. `lattice init` writes `.lattice/setup/` to `.gitignore` and
  keeps the legacy `.lattice-setup-marker` line
- Nothing under `.lattice` is walked for a cache key, so the marker no longer
  moves the key of a task that declares no `inputs`
- A marker Lattice cannot write produces a warning naming the real path:
  ``web: dependencies installed, but .lattice/setup/apps%2Fweb.marker could not
  be written, so the next `lattice setup` will install again: <io error>``
- `lattice setup <name>` with a name the config does not declare is refused. It
  used to select nothing and exit `0`, so a typo in a CI script looked like a
  clean run. The message is ``workspace 'X' is not declared in the `workspaces`
  array in lattice.json. Declared workspaces: api, web``
- `lattice setup` shows the installer's output as it runs, with no verbosity flag
  needed
- The installer no longer inherits the terminal's stdin. An installer that stops
  for a password or a confirmation now fails immediately, and its own output says
  what it wanted. It used to block on a prompt nothing displayed until the run was
  killed. Supply the credential through the environment, or run that installer
  once by hand outside Lattice
- `latticeVersion` is validated before it becomes a URL and a filename. A value
  that is not a version is refused by name instead of becoming a download of a
  release that cannot exist: ``lattice.json pins `latticeVersion` as "X", which is
  not a version. Write it like 0.2.0, or run `lattice upgrade <version>` to set
  it``. A leading `v` is accepted and stripped, so `"v1.0.0-beta-3"` and
  `"1.0.0-beta-3"` name one release, and either one naming the running build is a
  no-op rather than a failed download

### The desktop app shows output as it arrives — 2026-08-23

- Task output appears within about a tenth of a second. Output moved only when 256
  lines had piled up or a task changed state, so a dev server that prints one line
  and then serves requests never showed that line at all
- The graph fills in during a run rather than only once it ends
- Closing a project, or switching to a different one, stops that project's run and
  terminates its children. A run that kept going belonged to a project the window
  no longer showed, could not be stopped from the window, and reported task names
  from the wrong repo. Reopening the same project is a reload rather than a
  switch, and leaves the run alone
- Reloading the window reopens the project that was open and adopts a run still
  going in the backend, so the panes redraw and **Stop** comes back. It used to
  orphan that run
- A config save that failed no longer reports **Saved**. `useApp().saveConfig`
  resolves to a boolean saying whether the write landed
- A task the app stopped is no longer shown as a failure, because the runner no
  longer reports one

### The cache no longer hides a change behind a symlink, a `chmod`, or an empty output directory — 2026-08-23

The first run after this upgrade re-runs everything. Two of the fixes below widen
what a cache key covers, so every key that exists today moves.

- `inputs` pointing at a symlinked directory hashed zero files. The walk stopped
  at any symlink, so `inputs: ["vendor/**"]` against a symlinked `vendor`
  produced a key that could never change again, and the task hit cache forever
  after its first run. A symlinked directory is now descended
- Re-pointing a symlinked file left the key unchanged. A link now contributes the
  path it points at instead of the bytes on the other end, so switching
  `config/active.yaml` from `production.yaml` to `staging.yaml` moves the key even
  when both files hold identical content
- The two input walks treat a symlinked directory differently, on purpose. A
  declared `inputs` descends one, because the pattern names the files behind it.
  A task with no `inputs` does not, because that walk covers the whole workspace
  and following the link would pull an arbitrary tree from elsewhere on the disk
  into the key. Neither walk tracks where it has been. A depth cap of 64
  directories ends a cycle
- `chmod +x` on a hashed file did not move the key, so a hit restored the file
  without its executable bit. The artifact preserves file modes, so that bit is
  now hashed with every file's contents. Only that bit is hashed. The rest of the
  mode is umask and platform noise that would make a key depend on which machine
  computed it. Windows has no executable bit and reports every file as non-executable
- A task whose `outputs` matched only empty directories stored a valid artifact
  holding nothing. A bare `outputs: ["dist"]` matches `dist/` itself, so an empty
  `dist/` read as a result. Every later run then hit that entry, and the restore
  deleted whatever a real uncached run had produced. Lattice now refuses the
  store: `outputs ["dist"] matched only empty directories, so nothing was cached.
  Check that the task writes its files where the patterns point`. The existing
  `no files matched outputs [...]` message is unchanged
- A cache hit left behind directories the cached run never produced. Clearing
  before a restore skipped directories, so a `dist/chunks/` from an earlier build
  survived a hit from a run that never created it. Clearing now removes
  directories too, deepest first and only when they are already empty, so a
  matched directory still holding a file no pattern names is left alone
- Two `lattice` processes sharing one checkout could delete each other's cache
  writes. A store used to write its artifact before its metadata, and an artifact
  with no metadata is exactly what an interrupted store leaves behind, so the
  cleanup that runs at the end of every run under `settings.maxCacheSize` could
  not tell a live store from an abandoned one. A store now writes its metadata
  first, with no digest in it, and fills the digest in after the artifact lands
- A leftover artifact, staging file, or entry with no digest is reclaimed only
  once it has sat untouched for an hour. A store in progress and a store that
  died halfway through leave the same files behind, so age is the only evidence
  that separates them. Genuine debris now stays in the cache directory for up to
  an hour longer than before, and counts against `settings.maxCacheSize` while it
  does. A modification time in the future counts as recent, so a clock skewed
  between machines sharing a cache directory never causes a deletion
- An entry whose metadata records no digest is never a hit, and is never an
  eviction candidate. Its bytes still count toward the size budget

### `lattice.json` rejects the values it used to accept and then ignore — 2026-08-23

Each of these was previously accepted and then silently did something other than
what it said. Every one is now caught before any task runs.

- A key written twice in `tasks`, `engines`, a workspace's `engines`, or a
  workspace's `scripts` was last-wins and silent. In
  `{"tasks": {"build": {"outputs": ["dist/**"]}, "build": {}}}` the `outputs`
  declaration disappeared, so the task cached something different and Lattice
  said nothing about it. The message is ``duplicate key `build` in tasks
  (lattice.json line 1, column 58)``, then `Keep one of them: the second replaces
  the first, so only the last would take effect`. Only those four maps were
  affected. A repeated key anywhere else in the file was already rejected as a
  repeated field
- `timeout` has a maximum of 365 days, `31536000` seconds. An oversized value
  used to saturate to the largest representable number of seconds. That deadline
  overflows rather than arriving, so asking for a very long timeout produced no
  timeout at all. `assets/schema.json` gained `"maximum": 31536000`
  to match
- A `timeout` written as a fractional JSON number, such as `1.5`, is rejected
  rather than rounded up to `2`. The bundled schema has always declared that
  field an integer and the parser disagreed with it. The string form is
  unchanged: `"1500ms"` is still one second and `"1.5m"` is still ninety
- `settings.cacheDir` is validated. It has to be non-empty, relative, inside the
  repo, free of whitespace around a component, and not the repo root itself. It
  was unchecked before, so an absolute or `../` value put the cache outside the
  repo, and `"cacheDir": "."` pointed `lattice prune` at the top of the
  repository, where it deleted the `*.tar.gz`, `*.meta.json`, and `*.tmp` files
  it found
- An engine's `bin` is validated on the same rules, against the toolchain install
  directory. `{"bin": "/usr/bin"}` previously replaced the install path outright
  and went on the front of every task's `PATH`, while Lattice still reported a
  provisioned toolchain
- A `scripts` key that names no declared task is an error, with a `Did you mean`
  suggestion. A `scripts` entry only ever supplies the command for the root task
  of the same name, so a typo like `biuld` was accepted, stored, and never used.
  The workspace ran the command Lattice detected for it instead of the override
  written under `scripts`
- A workspace `path` with whitespace around a component is rejected. Windows
  strips that whitespace and unix keeps it, so ` apps/web` named two different
  directories depending on the platform
- Lattice now judges path containment by the running platform's own rules as well
  as by the text rules, so a spelling that resolves to a drive root or a
  filesystem root is caught even where the text rules do not enumerate it
- A path written with Windows separators, such as `apps\web`, still passes
  validation and still fails when the directory is resolved. On unix that is one
  filename rather than two components, so it cannot be judged an escape at load
  time

`assets/schema.json` and the committed `.lattice/schema.json` also describe the
`bin`, `timeout`, and `cacheDir` rules, so an editor validating against the
schema flags the same values the parser rejects.

### A persistent task that exits the moment it starts no longer hangs the run — 2026-08-21

`persistent: true` on a command that exits straight away could leave the run
waiting for a shutdown signal that was never coming. This is the typo case: a
dev server that dies on `port already in use` reported its exit, and then
nothing else happened until the caller gave up.

The runner counts persistent children to know when to stop holding the run open.
A child that dies instantly has its exit queued while its start is still
travelling back through the join handle, and `tokio::select!` takes whichever of
the two is ready without preferring either. When the exit won that race, the
counter was already at zero and clamped there. The start that arrived afterwards
raised it to one, and nothing ever brought it back down. The counter is signed
now, so both orders reach the same total.

The Windows installer job also stopped failing on a step that had already
succeeded. It piped through `tee /dev/stderr`, which Git Bash cannot open, and
`pipefail` turned that into a failed job even though the install worked.

### Raw output is `-v`/`--verbose`, and `-l`/`--loquacious` is the hidden alias — 2026-08-21

`--verbose` and `--loquacious` swapped roles. `-v`/`--verbose` is now the
documented flag that prints raw `workspace:task:` lines instead of the live
display. It is also the only spelling `lattice --help` lists.
`-l`/`--loquacious` still parses and still does the same thing, but no help
output mentions it.

Nothing else changed. `settings.loquacious` keeps its name in `lattice.json`,
and the flag and the setting still combine the same way: either one on its own
turns raw output on.

`-v` and `-V` now differ only in case. Lowercase prints raw output. Uppercase
prints the binary version and exists only on `lattice` itself, so
`lattice run -V` is still a parse error.

### Windows installs from a one-liner, and every published platform is built on its own hardware — 2026-08-21

`install.ps1` is a PowerShell installer, published next to `install.sh` on the
docs site and as a release asset:

```powershell
irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
```

It resolves a version the same way `install.sh` does — `$env:LATTICE_VERSION`,
then `latticeVersion` in `lattice.json`, then the newest release — verifies the
download against the release checksums, and writes
`.lattice\bin\lattice-<version>.exe` with a copy at `.lattice\bin\lattice.exe`.
The stable path is a copy rather than a symlink because Windows withholds the
privilege a symlink needs, which is what `lattice upgrade` has always done there.
It adds `.lattice\bin` to the user `PATH`, asking first when it has a terminal to
ask on, and skipping the edit entirely when it does not unless `-AssumeYes` is
passed. The `PATH` is read and written straight through `HKCU\Environment`
preserving the value kind, so a `REG_EXPAND_SZ` entry such as
`%USERPROFILE%\bin` is not silently frozen into a literal path, and `setx`, which
truncates at 1024 characters, is not involved.

`install.sh` no longer refuses Git Bash, MSYS2 and Cygwin. Those report a
`MINGW*`/`MSYS*`/`CYGWIN*` `uname`, and they are POSIX shells over a native
Windows filesystem, so the installer now targets `x86_64-pc-windows-msvc` there,
carries the `.exe` through every name, and copies rather than links the stable
path. It still turns away a bare `Windows_NT` and points at `install.ps1`. Under
WSL2 nothing changes: `uname` reports Linux and the Linux binary is what gets
installed, which is correct for a repo built from WSL2 and is not a Windows
install. On Windows on ARM both installers take the x64 build and say so, because
no `aarch64-pc-windows-msvc` build is published.

`x86_64-apple-darwin` is now built on `macos-15-intel` instead of `macos-latest`.
`macos-latest` has been arm64 since `macos-14`, so that target had been
cross-compiled and could not be run by the job producing it.

CI gained the platforms it was shipping blind. `build`/`test` now runs on macOS
arm64, macOS x86_64, arm64 Linux and Windows alongside the existing x86_64 Linux
job; before this, macOS was never built or tested at all and arm64 Linux was
first compiled during a release tag. `cfg(unix)` covers macOS at compile time and
hides nothing that differs at runtime. The desktop app builds on Linux, macOS and
Windows rather than Linux alone, since Tauri's webview is webkit2gtk, WKWebView
and WebView2 respectively. A new `installers` job runs `install.ps1` under
PowerShell and `install.sh` under Git Bash against a locally staged release,
which is the first time either installer has been executed by CI on any platform.

### Rust 1.88 is the floor, and `desktop` builds the app — 2026-08-21

`rust-version` in `[workspace.package]` is now 1.88, up from 1.86, and
`engines.cargo` in `lattice.json` follows it to `>=1.88.0`. Nothing in the code
needed it: `globset 0.4.20` raised its own `rust-version` to 1.88, and cargo
refuses to resolve a tree whose floor is below a dependency's. The badge, the
prose in the README, CONTRIBUTING and the installation page, and the CI MSRV job
all say 1.88. Two `%`-and-compare checks in `lattice-config` became
`u64::is_multiple_of`, which the raised floor made clippy ask for: the method
stabilized in 1.87, so the lint had been suppressed on the old floor. Both render
the same strings as before.

The `desktop` workspace declares its own commands, so a Lattice run drives the
Tauri app rather than only its frontend. `desktop:dev` is `npm run app`
(`tauri dev`), which opens the real window and starts Vite itself through
`beforeDevCommand`; `desktop:build` is `npm run bundle` (`tauri build`). Both
used to resolve to the detected npm scripts, `vite` and `vite build`, so
`lattice run dev` served the frontend at localhost:1420 and never opened a
window. Nothing is declared under `outputs`: `apps/desktop/src-tauri` is a member
of the root cargo workspace, so its artifacts land in the repo-root `target/`,
and an `outputs` glob cannot name a path above its own workspace.

### The CLI's messages, the desktop app's labels, and every doc — 2026-08-21

Two lines a script might grep for changed:

- `lattice: full cache — nothing to run` is now `lattice: full cache, nothing to
  run`
- the run header pluralizes for real. Where it used to end `across 3
  workspace(s)`, it now ends `across 3 workspaces`, and `across 1 workspace` for
  one

Help text and error messages are reworded throughout. Each error now names what
failed, where, and the one thing to do next, and the semicolons that joined two
clauses are periods. The unknown-field error, the ambiguity error, and the
`failed to <verb> <path>` context layers are unchanged: they already read that
way, and 20 assertions depend on them.

The desktop app labels its controls with the words the docs and `lattice.json`
use. `Where things can run` is `Workspaces`, `Files it reads` is `Inputs`, `Tool
versions to carry over` is `Engines`, and each carries one line underneath
explaining it. The 2026-08-08 entry below moved these labels the other way, to
plain English; a label that avoids the schema's word teaches nothing a reader can
search for, so they now use the real term and explain it in place.

The app calls the thing you open a project, everywhere. It used to say `repo` in
one control and `folder` in the next, for the same thing. The word `folder` is
gone from the app.

Three claims the app made were false and are corrected. The `--sequentially`
switch was labeled `One task at a time`, which is `--concurrency 1`; the flag
runs each task's graph to completion in turn, with normal parallelism inside each
graph. The engines help text said Lattice can fetch the tools it knows, which it
does only for an engine with an `installCmd`. The window's duration formatter
disagreed with the terminal's at three boundaries, so a 59.999s run read `60.00s`
in the window and `1:00` in the terminal.

The docs, the README, the website, and the published `skills/lattice` skill are
rewritten. Corrections worth naming, because each documented behavior the code
does not have:

- Ctrl-C on a persistent task sends `SIGTERM`, waits five seconds, then
  `SIGKILL`, and the run exits `130`. The docs said `SIGKILL` with no grace
  period and an exit of `0`
- `settings.maxCacheSize` takes a string. The docs gave a bare-integer form that
  the parser rejects
- driver detection takes the highest role rank across every candidate first, and
  only then uses a declaration to break a tie. The docs and the skill described a
  ladder that stops at the first rung, which told a reader that an `engines` entry
  always settles a driver conflict. Sometimes it does nothing
- a task's cache key includes the keys of the tasks it depends on, and a task
  with no `inputs` hashes its whole workspace minus gitignored files. Several
  pages described a smaller key
- the installer adds `.lattice/bin/` to `.gitignore` when the repo has one, and a
  `PATH` line to your shell config unless you pass `--no-modify-path`. The README
  said it touched nothing else, so the uninstall it documented left that line
  behind
- two error messages are built and then discarded, so no reader has ever seen
  them. `RunFailure` carries the keep-going summary and `RunInterrupted` carries
  `interrupted — running tasks were stopped`, and `lattice_project::run` takes the
  run result off each one and drops the message. A `--continue` run that fails
  prints its reporter summary and exits `1`. An interrupted run prints its summary
  and exits `130`. The docs quoted both messages as the run's final error, so they
  now describe what a reader sees, and the absence of an error line is stated
  rather than left as a surprise

### A copy pass over the changelog — 2026-08-21

- Entry headings and prose now describe what changed instead of making a case
  for it. No entry's facts, date, or version claim moved: this is a rewrite of
  how each change reads, not of what was released
- The references to the brand and messaging guides under `marketing/`, both
  deleted on 2026-08-21, are gone, so no entry sends a reader to a file that is
  not in the tree
- `apps/web/src/content/docs/changelog.md` reproduces the entries it keeps word
  for word rather than paraphrasing them. It leaves out the entries and
  paragraphs that only affect the repo, and adds nothing but the links into the
  docs. Two of those links pointed at anchors that no longer exist and now point
  at the page
- The comment under `## Unreleased` states the shape an entry takes

### The desktop app's labels, accent, and repo switcher — 2026-08-08

The app labeled its controls with `lattice.json` keys and CLI vocabulary:
`dependsOn`, `persistent`, "Ignore cache", "Concurrency: auto", "no driver
resolved". Each was accurate and meant nothing to a reader who had not read the
schema. The labels now read as plain English: "Waits for", "Keeps running until
stopped", "Skip the cache", "as many at once as fits", "nothing found to run it
with". Every control in the config form still shows the `lattice.json` key it
writes.

Strings the CLI also prints are unchanged. A task's state and a run's summary line
match the terminal's output character for character.

The accent color is crimson rather than ink, on the buttons, the checkboxes, the
focus rings, and the active rail. The lockup in the sidebar now carries the word
`desktop`. Failure moved off crimson to amber, so a broken row and a Run button
never share a color. Amber is a status color only.

`scripts/checkBrandTokens.mjs` compares the two copies of `bootstrap.scss`
setting by setting rather than byte by byte. The one setting the two copies must
disagree about is pinned, and every other setting still cannot drift. The
accent's `--focus` override lives in the app's own `_accent.scss`, so the token
file the website also uses is unchanged.

All thirteen ecosystems Lattice detects have artwork. Seven of them showed a
two-letter monogram before. The marks are vector rather than pre-sized PNGs, and
they sit on a fixed light plate, so a mark drawn for a white background is legible
in dark mode without a second file per ecosystem.

Opening a repo, scanning one, saving a config, and a run in flight each show a
spinner with a word beside it, in the accent color.

Switching repos was a two-glyph icon button in the bottom corner of the sidebar,
and it did not say what it switched. The open repo is now a dropdown at the top of
the rail, where its name already appeared. The dropdown lists every repo you have
opened, marks the current one, and holds the actions for opening another repo and
closing the current one.

### A desktop app — 2026-08-07

Lattice now has a window. It lists the tasks each workspace can run and runs them,
draws the dependency graph, edits `lattice.json`, and scaffolds a config for a
repo that has none. The app links the same engine crates the CLI links, in
process, so the two cannot disagree about what a task is or whether it needs to
run.

The app shows what a terminal cannot hold on screen: which workspaces exist and
what each one resolves to, the shape of the graph a task will run, and which of
the ten cache key components moved when a task misses.

Run it with `npm run app` in `apps/desktop`. This change ships no installers.

Making room for it moved code between crates. None of that changes CLI behavior.

`TaskEvent` and the `Reporter` trait now live in a new `lattice-events` crate,
which depends on `serde` and nothing else. `lattice-runner` no longer depends on
`lattice-output`, so watching a run no longer means linking a terminal stack.

A cache miss is now a typed `CacheMiss` value. It used to be collapsed into a
sentence where it was detected, which threw away the component list behind it. The
wording lives on the value now, so both front ends say the same thing.

The pipeline every front end runs moved out of the `run` subcommand into
`lattice-project`: open a repo, plan, execute, then interpret the result. The
subcommand held five `process::exit` calls, which is why nothing else could call
it.

`ExecuteOptions` gained `cancel`. `shutdown` reads like the way to stop a run, but
it is only awaited once the graph has drained and persistent tasks are holding the
run open, so a graph of ordinary tasks never consulted it. A caller that cannot
send itself a signal now has a way to stop a run.

The JSON Schema moved from the binary to `lattice-config`, next to the types it
describes.

### Cache entries live directly in the cache directory — 2026-08-07

Entries were written under a subdirectory named for an on-disk cache format
(`.lattice/cache/v3/`), and that name was hashed into every key, so a release
that changed what a key covered started a new group and left the old one for
`lattice prune` to reclaim. The running Lattice version already does that job. A
version bump moves every key, so the old entries are never asked for again. The
format directory bought only the directory sweep, which was the one part of prune
that ever called `remove_dir_all` on a path the user chose.

Entries now sit flat under `settings.cacheDir`, prune removes no directories at
all, and the first run after this upgrade re-runs everything. Anything left in a
`v*` directory from an earlier build is unreachable and safe to delete by hand.

### A declared env name reaches the key even when it is unset — 2026-08-07

Only resolved `(name, value)` pairs were hashed, so a name in `env` or `globalEnv`
that the environment did not answer contributed nothing at all. Adding one
therefore hit the entry computed before it was declared, and went on hitting once
the variable was set, because the value had never been part of the key to begin
with. The name is now hashed whether or not it resolves, with a set value and an
unset marker as distinct cases.

### A task command with a quote in it works on Windows — 2026-08-07

A task's command was handed to `cmd` as an ordinary argument. Rust quotes
arguments the way the MSVC runtime parses them, which escapes an embedded `"` as
`\"`. `cmd` does not read `\"` as an escape, so any command containing a quote
arrived mangled. `node -e "console.log(1)"` was enough, as was any path
with a space in it. Task commands, `installCmd`, `versionCmd` and the `setup`
installers all went through the same door.

Each now passes `/S /C "<command>"` as a raw argument, which is the documented
way to reach `cmd` verbatim: with `/S` it strips the first and last quote of the
rest and takes what is between them as written.

### The test suites run on Windows — 2026-08-07

Adding a Windows CI job showed that most of the suite could not run there, for a
reason that had nothing to do with what it covered: a test that drives a task
wrote its body as a POSIX shell script. `cmd` has no `;` separator, its `mkdir`
takes no `-p`, `echo hi > f` writes a trailing space, and `echo seed1>f` reads
the trailing digit as a file descriptor. Those tests were not testing Lattice on
Windows; they were failing on shell grammar.

Task commands now come from a dev-only `lattice-testkit` crate, which spells each
one for whichever shell will run it and is itself tested by executing every
command and checking its effect. The two stand-in tools the suites put on `PATH`
are small Rust programs rather than shell scripts, so they run wherever they were
built. Those two are a nested repo's task runner and a published release
binary.

`cargo test --workspace` now passes on Windows as well as unix, and the Windows
job runs the whole suite rather than a subset. Three tests stay unix-only on
purpose: two cover symlink round-tripping and one covers process-group teardown,
which are the platform's own mechanisms rather than something to emulate.

Two pre-existing suites were unix-only before this and are not any more:
toolchain provisioning, and the nested-repo passthrough pattern. Neither is a
unix feature, and provisioning is exactly the half where shell and `PATH`
handling differ.

### Shared files reach the cache key, and a run cleans up after itself — 2026-08-06

The first run after this upgrade re-runs everything: the key now covers more, so
it is a new cache format. The previous group is retired by the next prune.

- A file shared above the workspaces could not be covered by anything, so editing
  one served every task a stale artifact. `inputs` patterns are relative to the
  workspace and `tasks` is shared across workspaces, which left a base
  `tsconfig.json`, a shared schema directory or a root `.env` with no spelling
  that meant the same thing everywhere. Two root-level keys now cover them:
  `globalDependencies` (repo-root-relative globs) and `globalEnv` (variable
  names), both hashed into every task's key
- `lattice prune` deleted every directory beside the cache that was not the
  current cache format. With `cacheDir` pointing at a directory Lattice does not
  own outright, such as `.lattice`, that took `toolchains/` and `bin/` with it,
  including the installed binary. Only directories whose names have the shape of
  a cache format are reclaimed now
- Ctrl-C left every running task's children alive. Each task runs in its own
  process group, which is what lets a task that shells out be cleaned up as a
  unit, and the same call detaches it from the terminal's Ctrl-C. The signal
  reached Lattice, Lattice exited, and the compilers kept going. An interrupt now
  sends `SIGTERM` to every running group, waits up to five seconds, then kills
  what is left, and exits `130` rather than `1`, so a cancelled CI job does not
  report the same exit code as a build that failed
- A `dependsOn` that named nothing was a silent no-op. A workspace depending on a
  misspelled workspace name, or a task depending on a task the `tasks` map never
  defined, built no edge. The ordering the config was written to guarantee did not
  happen, and nothing was printed. Both are now rejected at load, with the nearest
  name offered
- A workspace `path` could be absolute or climb out of the repo with `..`. The
  workspace directory bounds which files are hashed, which the `outputs` globs
  match, and which a cache hit clears before unpacking, so a path that left the
  repo put all three somewhere Lattice has no business writing. Rejected at load
- `settings.maxCacheSize` was inert. It read as a budget but only `lattice prune`
  consulted it, so a repo that set one still grew without limit. Every run now
  holds the cache to it. There is no default, so a cache with no
  `settings.maxCacheSize` still grows without limit
- Toolchain provisioning could not work on Windows, which is a platform the
  release matrix publishes a binary for. Engine version checks and `installCmd`
  both ran through `sh -c` and joined `PATH` with `:`, so every engine in a
  config failed to resolve there. Both now use the platform shell and separator,
  the way the task runner already did, and CI builds and tests on Windows so it
  stays that way
- A task that hung had nothing to stop it, in CI as much as locally. Tasks accept
  a `timeout` (`"90s"`, `"10m"`, `"1h"`, or seconds); an overrun stops the task's
  whole process group and counts it as a failure. Ignored on a `persistent` task,
  and not part of the cache key
- A cache miss said only that it missed. The key is one hash, so it could not say
  what moved. It is now built from named components: `inputs`, `env`,
  `globalEnv`, `globalDependencies`, `dependencies`, `manifests`, `toolchain`,
  `command`, `patterns`, and `environment`. Each is recorded per workspace and
  task, so `-l` reports `cache miss: inputs changed` instead of a bare miss

### The cache key covers what actually determines a task's result — 2026-08-03

The first run after this upgrade re-runs everything. Entries are now grouped by
cache format on disk, and the previous group is retired by the next
`lattice prune` rather than read.

- Two workspaces could share one cache entry and restore each other's artifacts.
  The key did not include the workspace, so a task running the same command in
  two places with nothing else to distinguish it resolved to one identity. The
  second workspace reported a hit and unpacked the first one's build. The
  workspace name is now part of every key
- A change in a dependency did not reach the tasks that depend on it. `dependsOn`
  decided the order and nothing else, so editing a library rebuilt the library
  and then served its consumers from cache, against code that no longer existed.
  Every task's key now includes the resolved keys of its prerequisites. A
  workspace is one node, so a dependent re-runs when its dependency changes even
  if the specific files it reads did not
- A task with no `inputs` hashed no source files at all, so it cached on its
  first run and never ran again however much the workspace changed. Such a task
  now hashes its whole workspace, minus what the applicable `.gitignore` files
  exclude and minus its own `outputs`. Declaring `inputs` is now an optimization
  rather than a correctness requirement
- Only the invocation was hashed, never what the invocation resolves to.
  `npm run build` names a script in `package.json` and `make test` names a target
  in a `Makefile`; rewriting that script left the key unchanged and served the
  old artifact. Manifests present in a workspace are now hashed
- Lockfiles were only looked for beside the workspace, so every layout that
  hoists one to the top got no invalidation at all from a dependency bump. That
  covers pnpm, yarn and npm workspaces, and a Cargo virtual workspace. The repo
  root is now checked too
- The operating system, architecture and shell are in the key. A cache directory
  shared between runners, or between a host and a container, could answer one
  platform's lookup with another platform's artifacts
- A task's own outputs no longer feed its key. Previously an `outputs` glob
  inside the `inputs` set moved the key the run was about to store under, so the
  task could never hit its own entry; the workaround was to repeat every output
  in `ignore`. That is no longer necessary
- The `inputs`, `outputs` and `ignore` patterns are hashed as declared. Widening
  `outputs` used to leave the key alone, so the next run hit an entry that had
  captured the narrower set and silently restored less than the run produced

Storage got the same treatment:

- Artifacts and metadata are written to a temporary name and renamed into place.
  Nothing can read a half-written entry, and two `lattice` processes storing the
  same key can no longer interleave into one broken one
- A task that declares `outputs` and produces none of them is no longer cached.
  It used to store an empty archive, which verified correctly forever after, so
  the task reported a hit, restored nothing, and never ran again
- A restore clears what the entry's `outputs` match before unpacking. A file the
  task deleted stays deleted, and content-hashed names like `app.a1b2.js` no
  longer pile up across builds
- Symlinks are stored as symlinks and empty output directories survive. Symlinks
  were previously followed and flattened into copies of their targets
- `lattice prune` can see artifacts left without metadata by an interrupted run.
  It enumerated entries by metadata alone, so those bytes were invisible to it
  and could never be reclaimed, which made `maxCacheSize` unenforceable. It now
  also retires other formats' directories, and one unreadable metadata file
  evicts that entry instead of aborting the prune
- A directory symlink inside an input or output tree no longer recurses until the
  stack runs out
- An `outputs` pattern with no glob characters that names a directory, like
  `dist`, now captures that subtree. It previously matched nothing

`--no-cache` and `--force` are no longer the same flag. `--no-cache` neither
reads nor writes. `--force` skips the lookup but still stores, so it replaces a
suspect entry. It previously declined to write, so it could not.

### `lattice init` reads the repo instead of asking about it — 2026-07-31

- `init` opened by asking whether the repo needed Lattice as a build tool, a
  toolchain manager, or both. It then had you type every workspace name and path
  by hand and invent version constraints defaulting to `>=0.0.0`. It never
  looked at the directory it was scaffolding
- It now scans first. Every directory holding a manifest Lattice recognizes is
  proposed as a workspace, with its detected driver shown next to it. The walk
  skips hidden directories, gitignored paths, and dependency and output trees
- Tool versions the repo already records become proposed engines, at the version
  actually written down: `.tool-versions`, `.nvmrc`, `rust-toolchain.toml`,
  `.python-version`, `.ruby-version`, `.java-version`, `package.json`
  (`packageManager` and `engines`), and `go.mod`'s `toolchain` line
- The capability question is gone. On a terminal you get the two lists
  pre-checked and uncheck what's wrong. A repo root that holds only a workspace
  declaration is offered alongside its members but starts unchecked
- `init` no longer writes a config that does nothing: when the scan finds
  nothing, or you uncheck everything, it asks for at least one workspace or one
  engine first. Without a TTY there is no one to ask, so that case still writes
  the bare skeleton rather than failing a pipeline
- `--yes` takes the scan's proposal rather than always writing the skeleton
- A scanned `build` task only claims `outputs: ["dist/**"]` when a `package.json`
  workspace was found. A Rust or Go repo no longer gets a JavaScript convention
  written into its config
- A directory whose driver stays ambiguous is offered but starts unchecked, and
  is named before `init` exits. Declaring it would halt the next run, so `init`
  proposes a config that runs and reports what it held back

### A persistent task that exits is reported — 2026-07-30

- Lattice spawned a dev server and never looked at it again. A `persistent: true`
  task whose command exited left the run reporting it as running until Ctrl-C,
  then printing `0 failed`. A port already taken, or a one-shot command marked
  persistent by mistake, was enough. Every persistent child is now waited on
- An exit that isn't a clean `0` prints
  `web:dev: EXITED (code 1) after 1.09s` on stderr, counts in the run summary's
  failed count, and exits non-zero. A signal reads `EXITED (killed by signal)`
- An exit code of `0` prints the same line lowercased on stdout and counts as
  nothing
- A persistent task that has exited stops holding the run open. When the last one
  is gone the run prints its summary and exits instead of waiting for a Ctrl-C
  with nothing left to stop. Other persistent tasks still up are untouched, and
  the graph's scheduling is unchanged: a persistent exit stops nothing
- A child Lattice kills at shutdown is not reported and never counts as a
  failure. The kill request and the child's own exit can land in the same poll,
  so the shutdown flag decides that, not which one won
- On Unix, a persistent task that exits on its own now also takes down the rest
  of its process group, so a server the command backgrounded before quitting
  isn't left holding a port

### Breaking: an unknown key in `lattice.json` is an error — 2026-07-30

- The bundled schema has always set `"additionalProperties": false`, so an editor
  underlined a key Lattice does not define. The config types carried no
  `deny_unknown_fields`, so `lattice run` read the same file, ignored the key, and
  ran. The two now agree: every config type rejects a key it does not define, at
  every level
- This breaks any `lattice.json` carrying an extra key: a leftover `projects`
  map, a `settings.logging` from before it was removed, a `glob` on a workspace
  entry, or a note parked under a key of your own. Delete it. There is no
  opt-out
- The two typos this catches were both silent. `output` for `outputs` left a
  task declaring nothing to capture, so a cache hit restored no files. `input`
  for `inputs` hashed no files, so the task hit the cache after its first run
  whatever you edited
- The message names the key, the object holding it, its position, the nearest
  valid field, and everything accepted there:

  ```text
  Error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
  Did you mean `outputs`?
  Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache
  ```

  Containers read the way they do in the file: `tasks.build`, `workspaces[1]`,
  `engines.node`, or `at the top level of lattice.json`. The suggestion fires
  within one edit for short keys and two for longer ones, case-insensitively, so
  `Outputs` and `dependOn` are caught as well
- `engines` is hand-deserialized rather than an untagged enum. An untagged enum
  reports only that no variant matched, which would have buried the unknown key
  inside an engine object. A value that is neither form now says so:
  `invalid type: integer \`20\`, expected a version constraint string or an
  engine object`
- The pinned-version check still reads `latticeVersion` and `settings.versionCheck`
  straight out of the JSON, so a config written for a newer release can still say
  which build is allowed to read it
- A test asserts the schema and the config types accept the same key set at every
  level, so the two cannot drift apart again. The shipped-config tests now cover
  every `lattice.json` in the tree, examples included, rather than two of them

### A cache hit does not re-export the stored `env` — 2026-07-30

- `CacheStore::restore`'s rustdoc claimed the caller re-exports the entry's
  stored `env`. Nothing ever did, and nothing can: a hit starts no process, so
  there is no environment to export into. The runner's matching dead read is
  gone. It was an `entry.env()` assigned to `_cached_env`, under a comment that
  made it look deliberate
- The stored `env` stays. It is the record of the values the key was computed
  from, and since the key is a hash it is the only place they remain legible.
  `cache-internals.md` describes it that way instead of implying a hit re-applies
  it, and the page now states that restore overwrites the files at the output
  paths and touches nothing else
- Tests pin both halves: the values round-trip through the entry and survive a
  `touch`, `restore` leaves the process environment alone, and a stored entry's
  meta file records the resolved value that fed its key. The stress test
  asserts the same against a real `.meta.json`

### `--filter` runs what the filtered workspaces depend on — 2026-07-30

- A filter used to be applied before the graph was built, so a `^build` edge into
  a workspace the pattern excluded resolved to nothing. `lattice run build
  --filter lattice-runner` in this repo ran one task and reported success, having
  silently skipped the five workspaces it depends on
- The matched workspaces are now the roots of the run: the graph adds their
  transitive dependency closure, deduplicated, and drops everything else. A
  prerequisite whose inputs haven't changed comes back from cache, so the added
  cost is a cache lookup per node
- Nothing that depends *on* a match is included, so `--filter` still narrows a
  run to one part of the repo
- `--dry-run` tags each pulled-in node: `→ dagger:build (dependency)  cargo build`
- A workspace pulled in as a dependency is only asked for the tasks its
  dependents need, so an `auto: false` workspace outside the filter with no
  script for the task you named no longer halts the run. Toolchain provisioning
  and the `across N workspace(s)` count now cover the workspaces in the graph
  rather than every workspace declared

### The ambiguity error suggests a fix that works — 2026-07-30

- When a workspace had no ecosystem marker, the halt suggested
  `"engines": { "node": ">=0.0.0" }`. Pasting that in reproduced the same error,
  because a runtime cannot drive named tasks. It now suggests
  `"auto": false, "scripts": { "build": "<command>" }`, which resolves it
- The `node` fallback fired whenever the candidate list was empty, so the two
  workspaces most likely to hit it were the emptiest ones: a directory holding
  only a `.nvmrc`, and a directory with nothing Lattice recognizes at all
- Where a candidate tool does exist, nothing changes. Every tool a generic
  ecosystem marker maps to can drive, so a bare `package.json` still suggests
  `pnpm`. A test asserts that stays true as the marker table grows
- The stress test now pastes the suggested fix back in and asserts the run
  succeeds

### A voice and accuracy pass over the docs — 2026-07-30

- A voice pass over all 28 pages of `apps/web/src/content/docs/`. Removed the
  framing sentences that announced what a section was about to say, the
  commentary on our own choices (`and that is deliberate`, `by design`,
  `is the point`), and the bolded-lead-in bullet list used as a default
  structure. Prose where it reads better, tables where the content is a table
- `environment-variables.md` no longer tells you to prefer flags over variables.
  Both are supported, the flag wins where both are given, and the page says so
  once instead of organizing itself around it. The column headed
  `Flag to prefer` is now `Flag`, and the section justifying the preference is
  replaced by one explaining the version-switch handover, which is why
  `LATTICE_SWITCHED_FROM` and the release-URL overrides have no flag
- The same ranking had spread to `output-modes.md` and
  `continuous-integration.md`, where `CI` and `-l` are now two triggers for one
  mode rather than a recommendation
- Source citations are gone from every page. They pointed at line numbers that
  had already drifted in two crates, and the file a message comes from is not
  something an end user needs. `architecture.md` keeps the crate layout, since
  there the directory structure is the subject
- Eleven factual errors fixed along the way. `drivers.md` had driver selection
  backwards: role rank decides, and a declared engine only breaks ties within
  the winning role, so `"engines": { "node": ">=20" }` beside a
  `pnpm-lock.yaml` resolves to pnpm. `glossary.md` already described this
  correctly, so the two pages disagreed. Also: an unparseable cache meta file
  surfaces as a warning and a re-run, not the silent miss `cache-internals.md`
  described; `DriverSpec` has a `roles` slice, not one `role`; raw-mode
  `skipped` lines are `-l`-gated; interactive mode discards task output rather
  than streaming it; `web` does define a `test` script; `init` prompts on a
  terminal without `--yes` and never consults stdin; a `glob` key in
  `workspaces` parses and does nothing rather than being rejected
- `errors.md` gained the archive and release-download messages it was missing
- `compute_key`'s rustdoc had the same imprecision the docs did: input files and
  env pairs are sorted before hashing, lockfiles are visited in `LOCKFILES`
  order

### The multi-language example declares a cross-language edge — 2026-07-30

- `examples/polyglot` declared `"build": { "dependsOn": ["^build"] }` while no
  workspace in it declared `dependsOn`. `^build` expanded to nothing, all four
  workspaces were independent roots, and the example demonstrated none of the
  cross-language ordering it exists to show. `worker` (Go) now declares
  `"dependsOn": ["utils"]` (Python), so `utils:build` runs first
- A test asserts the pair holds: if the example keeps a `^task` but loses every
  workspace edge, `cargo test -p lattice-config` fails
- The walkthrough at `apps/web/src/content/docs/multi-language-monorepo.md` no
  longer demonstrates the edge as a hypothetical addition. The run output on that
  page is recaptured against the example as it now ships, as is the quick start
  in the README

### `settings.logging` is gone — 2026-07-30

- The field validated against the bundled schema and changed nothing. Nothing in
  the tree read it. It is removed from the config type and from the schema
- Output verbosity is `-l`/`--loquacious`, `settings.loquacious`, and `CI`, which
  is what it always was
- A `lattice.json` still carrying `logging` keeps loading, because an unknown
  setting is ignored rather than rejected, so nothing breaks on upgrade. Editors
  pointed at the refreshed `.lattice/schema.json` will start flagging the key.
  Delete it
- `.lattice/schema.json` is only written when absent, so a repo initialized
  before this keeps its copy. Delete the file and run any `lattice` command to
  pick up the current one

### Flags for what used to be environment variables — 2026-07-29

- `--theme light|dark` replaces `LATTICE_THEME`, and picks the teal shade of the
  splash art. A value that is neither is now a parse error listing the two that
  work, rather than a silently ignored string. It is global, so it parses on
  `lattice` itself and on every subcommand
- `--release-base-url <URL>` replaces `LATTICE_RELEASE_BASE_URL`. Also global,
  because `upgrade` is not the only thing that downloads. An invocation in a repo
  pinning a version that is not installed fetches it under whatever command you
  typed
- `--release-latest-url <URL>` and `--release-list-url <URL>` replace
  `LATTICE_RELEASE_LATEST_URL` and `LATTICE_RELEASE_LIST_URL`. These sit on
  `lattice upgrade`, the only command that resolves `latest`
- Every one of those variables still works. The flag wins where both are given,
  and a blank value at either step falls through to the default rather than
  building an empty URL
- `LATTICE_SWITCHED_FROM` stays a variable on purpose. It is read by the process
  a version switch hands the invocation to, and that process is a different
  build of Lattice. An older build would reject a flag it has never heard of and
  fail the handover. For the same reason, a repo pinning a version older than
  these flags should keep exporting `LATTICE_RELEASE_BASE_URL`: the handover
  passes the whole command line through, so a flag the pinned build does not
  know reaches it as an error
- `LATTICE_TOOLCHAIN_DIR` is unchanged and gets no flag. It is what Lattice hands
  to an engine's `installCmd`, not something you tell Lattice

### One color per task in the plain stream — 2026-07-29

- The `workspace:task` label leading every line of the plain stream is now
  colored, one color per task, so the interleaved output of a parallel run can be
  followed by eye. `web:build`, `web:test`, and `api:build` are three different
  colors; the eight in the palette are one hue step apart at a fixed saturation,
  and none of them reads as the red a `FAILED` marker uses
- Colors are handed out in the order labels are first seen, so the first eight
  distinct labels in a run never share one. Because tasks start in parallel, which
  color a task gets can differ between runs. Within a run it never changes
- The loquacious trace lines carry the same colored label, so `lattice:
  web:build: hash …` and `web:build:`'s own output read as one stream
- Whether color is emitted now depends on stdout being a real terminal rather than
  on which mode you got. `-l` at a shell colors labels; the same run piped,
  redirected, or under `CI` emits no escapes at all and is byte-for-byte what it
  printed before. `NO_COLOR` still suppresses everything
- Nothing but the label is styled, and `FAILED` is still the word `FAILED`

### A run that executes nothing says so — 2026-07-29

- When every task in a run comes back from cache, the summary is followed by
  `❖❖❖ FULL CACHE`, painted across the teal ramp a character at a time. Plain
  output gets the same signal without color, as `lattice: full cache — nothing
  to run`, so a CI log can be grepped for it
- It requires at least one scheduled task, zero failures, and zero tasks that
  ran, so a filter that matched nothing stays quiet and a graph with a
  `persistent: true` task never qualifies

### The toolchain table, filled in — 2026-07-29

- CocoaPods, pip, NuGet, and Kotlin are now fully wired rather than half-known.
  `pod` had a driver row but no engine rule and no installer; `pip` had an
  installer that no driver could ever reach; `nuget` and `kotlin` were absent.
  All four are drivers, well-known engines, and known to `lattice setup` now
- `deno` and `bun` are runtimes as well as a task runner and a package manager,
  and `mix` is a package manager as well as a task runner. A driver declares
  every role it fills and competes with its highest-ranked one, so what drives a
  workspace is unchanged: `deno` still drives as a task runner and `bun` as a
  package manager. The table now lists every role a tool fills
- The well-known engine list and the built-in version commands were two separate
  tables that disagreed. `uv`, `poetry`, `just`, `turbo`, `nx`, `swift`, `dart`,
  `composer`, `mix`, `stack`, `cabal`, `pdm`, and `pipenv` all had a version rule
  but were rejected in string form, so `"engines": { "uv": ">=0.5" }` failed to
  load for no reason. They are one table now, in `lattice-config`, and every
  driver is guaranteed a row in it
- `"python3": ">=3.12"` was checked by running `python --version`, which on many
  machines is a different interpreter, or Python 2. It runs `python3 --version`
- `lattice setup` knows how to install dependencies for 11 more drivers:
  `dotnet restore`, `nuget restore`, `pod install`, `swift package resolve`,
  `composer install`, `mix deps.get`, `dart pub get`, `pdm install`,
  `pipenv install`, `stack build --only-dependencies`, and
  `cabal build --only-download`. A `.csproj` workspace used to report "no known
  dependency installer" and skip
- 13 more lockfiles feed cache keys, including `deno.lock`, `composer.lock`,
  `mix.lock`, `pubspec.lock`, `Package.resolved`, `Podfile.lock`,
  `packages.lock.json`, `pdm.lock`, `Pipfile.lock`, and `requirements.txt`. A
  dependency bump in those ecosystems used to hit a stale cache entry. The cache
  and `setup` read one shared list, so they can't drift apart
- `npm-shrinkwrap.json` is npm evidence, alongside `package-lock.json`
- An ambiguity error suggests better candidates. A bare `Cargo.toml`, `go.mod`,
  `composer.json`, `mix.exs`, `pubspec.yaml`, `Package.swift`, `stack.yaml`,
  `cabal.project`, or `.csproj` used to produce an empty candidate list; each
  now names the tools that could plausibly have been meant
- Two drivers have no fingerprint on purpose. A `requirements.txt` is read by
  pip, uv, and pip-tools alike, and a Kotlin workspace is driven by gradle or
  maven, so `pip` and `kotlin` are selected by declaration rather than by
  guessing. For the same reason `packages.lock.json` is not nuget evidence: an
  SDK-style project can carry one and still be a `dotnet` workspace

### An agent skill for `lattice.json` — 2026-07-29

- New `skills/lattice/`: a `SKILL.md` plus four references. The references cover
  the `lattice.json` fields, the CLI surface, the driver and engine tables, and
  symptom-to-fix troubleshooting. Install it with
  `npx skills add latticeandcompany/lattice`, or load it for one session with
  `npx skills use latticeandcompany/lattice@lattice`
- It leads with the traps an agent actually falls into: a persistent task blocks
  until it is interrupted, `--filter` never pulls in dependencies, a task with no
  `inputs` hits the cache after you edit its source, and a typo'd key in
  `lattice.json` is silently ignored by the parser
- The skill is symlinked into `.agents/skills/` and `.claude/skills/`, so the
  agents working on this repo use the copy we publish rather than a second one
  that drifts
- The hero's copyable command is now a two-tab control: `Install` by default,
  `For agents` for the skill. New `/for-agents` page, in the navbar and the
  footer, covers what each file in the skill contains and how to load it for a
  single session
- The stress test now checks the skill against the binary: every subcommand and
  long flag in `--help` has to be documented and vice versa, every engine the
  skill calls well-known has to be accepted in string form (and every one it
  doesn't, rejected), and every Build Tool row of its driver table is verified
  from its own fingerprint through `--dry-run`

### Search and repo link fixes, and a Get started page — 2026-07-29

- Search results 404'd. Pagefind derives its own base by stripping `pagefind/`
  off the path its bundle loads from, so the URLs it hands back already carry
  the site's `/lattice` prefix; the dialog was adding it a second time and
  sending readers to `/lattice/lattice/docs/…`. It now passes those URLs
  through untouched
- The footer's License link pointed at `blob/main/LICENSE`, and this repo's
  default branch is not `main`. Repo links resolve through `blob/HEAD` now, so
  they survive a branch rename
- New `/get-started` page: six steps from an empty repo to a build that comes
  back from cache, with every terminal transcript captured from a real run
  rather than written by hand. Every "Get started" button on the site points
  there instead of at the docs
- The footer drops to two columns of destinations someone would actually look
  for. Deep docs pages belong in the docs sidebar, not the footer
- `npm run build` now ends in a link check that fails on any internal link or
  indexed search result that would 404 under the site's subpath, or any repo
  link that pins a branch name. The URL shaping behind the search dialog moved
  to `src/lib/search.ts` and has unit tests (`npm test` in `apps/web`)

### Dependency bumps, and a Dependabot auto-merge that works — 2026-07-28

- `displaydoc` 0.2.6 → 0.2.7
- The docs site's Astro stack moves to `astro` 7.1.5, `@astrojs/mdx` 7.0.5 and
  `@astrojs/react` 6.0.2, with `sharp` 0.35.3 and the usual transitive patches
  underneath
- The Dependabot workflow asked for `gh pr merge --auto --squash`, which this
  repo does not allow, so every auto-merge attempt failed and nothing was ever
  merged automatically. It now asks for a merge commit
- Dependabot writes `package.json` with CRLF, which `.gitattributes` normalizes
  away on checkout and leaves the file permanently modified. Renormalized

### Installing, upgrading, and running the version a repo pins — 2026-07-28

Lattice can now be installed without a Rust toolchain, and a repo's
`latticeVersion` is enforced rather than merely announced.

#### `curl | sh` installs a target-matched binary into the repo
- `apps/web/public/install.sh` detects the OS, architecture and libc, resolves a version,
  downloads the matching release archive, verifies its SHA256 against the release's
  checksums file, and installs `./.lattice/bin/lattice-<version>` with
  `./.lattice/bin/lattice` symlinked to it. Nothing is written outside `.lattice`,
  so `rm -rf .lattice` is the uninstall
- Version resolution, in order: `$LATTICE_VERSION`, then `latticeVersion` from
  `./lattice.json`, then the newest release when the directory has no config at
  all. The pin is read by the installer because it has to be known before a binary
  exists to read it. A `lattice.json` that exists but pins nothing is an error
- It fails loudly, before installing anything, on an unsupported platform, a
  missing pin, a missing asset, a missing checksums entry, or a digest that does
  not match
- It sits in the docs site's `public/`, so the site serves it verbatim at
  `latticeandcompany.github.io/lattice/install.sh` with no build step of its own.
  The release workflow publishes the same file as a release asset
- Keeping versioned binaries on disk is what makes a branch switch cheap, so
  `.lattice/bin/` is now in the `.gitignore` lines `lattice init` maintains

#### Every invocation runs the version the repo pins
- A binary under `.lattice/bin` whose version differs from `latticeVersion` now
  prints one line naming both versions, installs the pinned version if it is not
  already on disk, repoints the symlink, and hands the invocation over to it with
  the arguments untouched. Switching between two branches that pin versions you
  already have is a symlink swap and touches no network
- The pin is read straight out of the JSON rather than through the config loader.
  A config written against a newer schema has to be able to say which version can
  read it, so the handover happens before anything that could reject it
- A binary Lattice did not install is never replaced, whether it came from
  `cargo install`, a distro package, or `scripts/dev-link.sh`. Those keep the
  advisory one-line nag from #45, which now prints a runnable
  `lattice upgrade <version>`
- `--no-version-check`, `LATTICE_NO_VERSION_CHECK` and
  `settings.versionCheck: false` each skip the whole thing. `upgrade`, `version`
  and `completions` are never handed off: they answer for the binary that was
  invoked, and a completion script has to be the only thing on stdout
- A pinned version that cannot be installed is a hard failure naming the version
  and the way past it, rather than a run on whichever binary happens to be
  present

#### `lattice upgrade <version|latest>`
- Installs the version, points `.lattice/bin/lattice` at it, and rewrites
  `latticeVersion`. `latest` resolves the newest release; a bare version pins it
  exactly, with or without a leading `v`
- The config is edited as text, so key order, indentation and the rest of the file
  survive a bump. A version that is not a version is rejected before it can reach
  a URL or a filename
- Re-running for a version already pinned and installed reports that and repoints
  the symlink, which is the one case where doing nothing would leave the repo on
  the wrong binary

#### Releases are built and published by tag
- `.github/workflows/release.yml` builds `v*` tags for six targets: macOS
  x86_64 and aarch64, Linux x86_64 gnu and musl, Linux aarch64, and Windows
  x86_64. It publishes `lattice-<version>-<target>.tar.gz` archives carrying the
  binary, the license and completion scripts, plus one
  `lattice-<version>-checksums.txt` and the installer
- Completions are generated once on a native runner, because a cross-compiled
  binary cannot be run to print its own
- The tag has to agree with the tree: `scripts/check-versions.sh <version>` gates
  the build, and CI now runs it on every push, along with `shellcheck` over the
  installer

#### `check-versions.sh` never actually passed
- Both value extractions were `gsub(/.*"|".*/, "")`, whose first alternative is
  greedy enough to match through the closing quote and delete the value along with
  the key. Every version read came back empty. It now splits on `"` and takes the
  field, which is what that line was reaching for
- The same script compared `name = "lattice"` against lines checked out CRLF by
  `.gitattributes`, so all seven `Cargo.lock` assertions failed on a fresh clone.
  Every read now strips the carriage return first
- With it running, its own README rule had something to say: the hardcoded
  `badge/version-0.1.0` shield is replaced by `github/v/release`, which cannot go
  stale between releases

### The endorsement footer pointed at a placeholder org — 2026-07-28

- The brand guide under `marketing/` still shipped `https://github.com/<org>` in
  the donated-project endorsement footer, in both the prose form and the
  copy-paste HTML. Any project that followed it would have published a dead link.
  Each copy now points at `https://github.com/latticeandcompany`, matching the
  `repository` in `Cargo.toml`

### Four documented promises the code did not keep — 2026-07-28

Groundwork for the first tagged release. Each item here was a statement in the
README, the docs or a manifest that the code contradicted.

#### The stated minimum Rust version was wrong by eleven minor versions
- `Cargo.toml` declared no `rust-version` at all, while the README badge, the
  README Development section, `CONTRIBUTING.md` and `lattice.json`'s
  `engines.cargo` all claimed 1.75. Resolving the lockfile against each
  dependency's own `rust-version`, the real floor is 1.86: `clap`, `sha2` and
  `indexmap` need 1.85, and the ICU crates reached through `jsonschema` → `idna`
  → `idna_adapter` need 1.86. A build on 1.75 could not have worked
- `rust-version = "1.86"` is now set once in `[workspace.package]` and inherited
  by all seven crates, and the four prose copies are corrected to match.
  `cargo +1.86 check --workspace --all-targets --locked` passes
- Declaring it turned on clippy's `incompatible_msrv` lint, which found one real
  violation: `u64::is_multiple_of` in `lattice-config` is stable since 1.87. It
  is now `bytes % unit == 0`, which is what the method is sugar for, so the
  floor stays where the dependency tree actually puts it
- `jsonschema` is a dev-dependency taken with `default-features = false`. Its
  default features pull `reqwest` in for remote `$ref` resolution, which the
  schema tests never use. That dragged a whole TLS stack into the dev tree,
  against `CONTRIBUTING.md`'s rule about network access. All 23 schema tests
  pass without it

#### `version --json` could not report a target triple
- The `target` field was `std::env::consts::ARCH`, so it printed `aarch64`
  rather than `aarch64-apple-darwin`. `bug_report.yml` asks contributors for
  that output to identify a platform, and the installer needs the same
  vocabulary to pick a release asset
- A `build.rs` on `crates/lattice` now emits `LATTICE_TARGET` from cargo's
  `TARGET`, and the bare architecture moves to its own `arch` field rather than
  being dropped
- The stress test asserted only that a `target` key existed, which a bare arch
  satisfied. It now also asserts the value looks like a triple, and a missing
  `version` field is a hard failure instead of silently falling back to a
  hardcoded `0.1.0`

#### A docs page described a command that does not exist
- `apps/web/src/content/docs/templates.md` documented scaffolding new workspaces
  from a template. There is no `templates` command in
  `crates/lattice/src/commands/`, and `CONTRIBUTING.md` prohibits documenting
  features that do not ship. The page is deleted and `nested-repos.md` moves up
  to fill the gap it left in the Guides ordering

#### Windows was presented as merely untested
- `lattice-runner` has a correct `cmd /C` branch, but `lattice-workspace`'s
  toolchain probe hardcodes `sh -c` and joins `PATH` with `:`, so engine version
  checks and toolchain provisioning cannot work there. A Windows binary would
  launch, print a splash, and fail at the detection ladder
- The README and the getting-started page now say macOS and Linux, and point
  Windows users at WSL2

#### Manifest metadata
- `[workspace.package]` gains `repository`, `homepage`, `documentation`,
  `keywords` and `categories`, none of which any crate carried, plus
  `publish = false` so that a stray `cargo publish` cannot push. Five of the
  seven crate names are unclaimed on crates.io, so without that guard a mistyped
  `-p` would permanently publish an unsupported library. `lattice` and `dagger`
  are both taken by unrelated projects
- `[profile.release]` sets `strip`, thin LTO and one codegen unit, since release
  output is now something we ship rather than something we only build locally

### Repo hygiene pass — 2026-07-28

#### Line endings corrupted committed binaries
- `.gitattributes` was a single `* text eol=crlf` rule. Because `text` was *set*
  rather than auto-detected, git applied CRLF normalization to binary files as
  well. A PNG's 8-byte signature contains a `CR LF` pair, so normalizing one
  corrupts it: `.github/assets/latticeco-black.png`, `latticeco-white.png` and
  `apps/web/src/assets/languages/node-dark.png` were all committed damaged. The
  two company logos were unreadable in the README
- The rule also gave every shell script a CRLF shebang, which makes it
  unrunnable (`env: bash\r: No such file or directory`). `scripts/dev-link.sh`,
  `scripts/dev-unlink.sh`, `scripts/stress-test.sh` and
  `examples/nested-repo/services/api/src/serve.sh` could not execute, and the
  README documents two of them as the way to work on the repo
- `.gitattributes` now lists authored text extensions explicitly, holds `.sh`
  and `.txt` at LF, and marks binary formats `binary` so no future `text` rule
  can reach them. `rosette.txt` is included with `include_str!` and printed to a
  terminal, which is why it is LF
- `node-dark.png` is repaired from its intact working-tree copy. The two
  `latticeco-*.png` logos were damaged in both the blob and the working tree,
  with no earlier commit to recover from, so they are re-exported from
  `marketing/lattice-and-co/lockup-horizontal-*.png`. They are now 480px wide
  with an alpha channel, so the dark-theme copy no longer renders as a black box
  on GitHub

#### Rust formatting
- Added `rustfmt.toml` with `hard_tabs = true`. Rust was 4-space while
  `.editorconfig` and `CODESTYLE.md` both call for tabs; 21 files are
  reformatted. Every other rustfmt setting stays default

#### CODESTYLE.md
- The file was ArenaSwap's, copied verbatim. It mandated camelCase filenames and
  single quotes repo-wide, listed `wxt.config.ts` and `turbo.json` as protected
  filenames that do not exist here, and gave no Rust guidance at all in a repo
  that is mostly Rust
- It now splits into a Rust section (`crates/`) and a Web section (`apps/web/`),
  with the shared principles kept common. The Rust section covers rustfmt and
  clippy as gates, naming, crate layout, `anyhow` error context, and the
  `//!`/`///` rules that `AGENTS.md` already implies. `dagger` is recorded as
  the one deliberate exception to `lattice-<role>` crate naming, and PascalCase
  Astro layouts are recorded as the exception to camelCase filenames

#### Ignored and untracked
- `.gitignore` now covers `.DS_Store`, `.idea/`, `.vscode/` and `*.log`. Three
  `.DS_Store` files were sitting in the repo, kept out only by a machine-local
  global excludes file
- `dist/` and `.astro/` are no longer scoped to `apps/web`; running the site
  from the repo root drops a `.astro/` at the root instead
- Running the examples dirtied the repo, which CONTRIBUTING tells contributors
  to do. `examples/polyglot/apps/web/dist/index.html` and
  `examples/polyglot/libs/utils/dist/utils.py` were committed build outputs and
  are now untracked; the ignore list covers every example's `dist/`, `target/`,
  `.venv/`, `bin/` and `__pycache__/`

#### GitHub
- Added `.github/dependabot.yml` covering the Cargo workspace, the docs site's
  `apps/web` lockfile, and GitHub Actions, on the same Friday cadence and
  `Upgrade` commit prefix the other repos use
- Added `.github/workflows/dependabot-automerge.yaml`. It listens on
  `pull_request` only. The ArenaSwap original also listens on
  `pull_request_target`, which runs every step twice and hands `contents: write`
  to a fork-triggered workflow. Patches auto-merge. Minors auto-merge for the
  docs stack and the crates the test suite exercises, while anything touching
  caching, hashing or process control waits for review
- Added `.github/copilot-instructions.md` pointing at `AGENTS.md`
- Deleted `.github/assets/lockup-black.png` and `lockup-white.png`, which
  nothing referenced. The README uses the SVGs
- README gains the CI, stars, forks, issues and last-commit badges the other
  repos carry

#### marketing/
- Filenames are kebab-case throughout: `lattice_icon_black.svg` →
  `icon-black.svg`, `favicon_black.svg` → `favicon-black.svg`,
  `ascii-art_full.txt` → `ascii-art-full.txt`, and the `_lockup` variants →
  `lockup-black.svg` / `lockup-white.svg`
- `Lattice & Co/` had a space and an ampersand in its path, mode 700, and six
  files named `1.png` through `6.png`. It is now `lattice-and-co/` at mode 755,
  with the marks named for what they are: `lockup-horizontal-*`,
  `lockup-stacked-*`, and `monogram-*`
- The brand guide's asset table is updated for the new names and gains the rows
  it was missing: `pattern.svg`, both ascii-art files, and the company marks. A
  new "Where each copy lives" section explains why the same lockup exists in
  `marketing/`, `apps/web/public/brand/` and `.github/assets/`. The copy in
  `marketing/` has a live `<text>` wordmark and is the editable source. The
  other two are outlined so they render without DM Sans installed

#### Stale copy
- Root `lattice.json` listed `tailwind.config.cjs` as a build input. The file
  does not exist; the docs site is Tailwind v4 through the Vite plugin
- `examples/polyglot/apps/web/package.json` described itself as a Next.js app.
  It is an `echo` and `mkdir` script

### Long durations print as a clock — 2026-07-28

- Task and run times over a minute now read `4:07` and `1:12:30` instead of
  `247.00s` and `4350.00s`. Under a minute is unchanged (`1.23s`). Applies
  everywhere `lattice` prints a duration: per-task completion lines and the run
  summary, in both the interactive and CI reporters

### Copy and comment cleanup across the repo — 2026-07-28

#### Voice guides
- The voice section of the brand guide is now an enforceable contract rather
  than three bullets. It names the banned words, the pillar phrasings that must
  not ship, and the slop patterns that kept reappearing: bolded-lead-in bullet
  lists, rhythmic triads, unmeasurable comparatives, invented numbers, em-dash
  dramatic pauses, and aphorisms about ourselves
- The voice table previously used the banned word "polyglot" in its own
  recommended column, and offered invented benchmarks ("Cold builds in 4s.
  Cached in 90ms.") as the model answer in a section requiring measurable
  claims; both are corrected
- The messaging guide's value props were named after the four core pillars,
  which the guidelines forbid. They are renamed for what the user gets
- The official tagline is recorded as an explicit, documented exception to the
  performance-adjective and pillar rules, so a future pass does not "correct" it
- `.claude/agents/lattice-docs.md` prescribed `### Added/Changed/Fixed` sections
  with bold lead-in bullets, which is the style the entries above were written
  in. It now carries the current format

#### Copy
- The tagline is one string in 12 places, including the `crates.io` package
  description in `crates/lattice/Cargo.toml`. Two competing variants were in use
- "polyglot" is gone from all prose; the `examples/polyglot/` path is unchanged
- Every `https://lattice.build` URL is replaced. The domain is not registered,
  so the documented install command and all five docs links were dead
- Installation now documents the from-source path that works. The advertised
  `curl | sh` one-liner had no installer behind it at any URL, and there is no
  release to fetch a binary from
- GitHub URLs are normalized to the `latticeandcompany` org; the repo previously
  used two owners across 12 links

#### Crates
- 67 dash-padded banner comments removed across 8 files, including the 22
  `// ---- x tests ----` markers inside modules already named `mod tests`
- Nineteen `(PRD §…)`, `(BRAND.md §…)`, and `(decision #…)` citations are gone
  from the crates. Each pointed at a document that had already been deleted
- Redundant doc comments that restated the item's name are deleted, along with
  comments narrating past fixes or congratulating the code
- All-caps prose emphasis, rhythmic triads, and pillar bragging removed from
  module headers and doc comments
- User-facing error messages follow one convention: lowercase start, no trailing
  period
- Four stress-test assertions and four unit-test assertions updated to the new
  message casing

#### Docs
- The orphaned 228-line `docs/toolchains.md` is folded into the published
  `apps/web/src/content/docs/toolchains.md`, which was a 14-line stub pointing
  back at the repo file, and the root duplicate is deleted
- `docs/` no longer exists; the published docs are the single source

### Public-facing repo documentation — 2026-07-28

#### Repo
- `.github/README.md` leads with the lockup and the tagline, the install
  one-liner, and a quick start whose terminal output is captured from a real run
  of `examples/polyglot`: a cold run, then the same run entirely from cache
- `CONTRIBUTING.md` covers the crate layout, the `feature/* → mega` flow, the
  build and test commands, dogfooding through `scripts/dev-link.sh`, and the
  Rust standards: clippy clean at `-D warnings`, no `unwrap`/`panic!` on
  user-reachable paths, errors propagate
- The testing policy requires tests with every change, the stress test updated
  and exiting `0`, and cache work proved in both directions
- The AI-assistance policy accepts agent-written contributions on the condition
  that they are disclosed and that the human author owns every line
- `CODE_OF_CONDUCT.md` sets out the enforcement ladder, the prohibited-conduct
  list, and appeals routed to the maintainer
- `SECURITY.md` routes reports through private GitHub advisories and states the
  threat model: executing declared commands, running detected tools, and
  provisioning a declared `installCmd` are intended behavior, while cache
  poisoning, extraction path traversal, escapes from `./.lattice/`, distribution
  integrity failures, and leaked secrets are in scope

#### Issue and pull request templates
- Four YAML forms in `.github/ISSUE_TEMPLATE/`; blank issues are off and
  questions route to Discussions
- Bug reports collect `lattice version --json`, the platform, the relevant
  `lattice.json`, the exact command, and `-l` output
- Feature requests ask whether the thing is already possible by declaring the
  commands yourself, and gate on not requiring Lattice to identify a tool it has
  no evidence for
- The toolchain form asks for what a new driver actually needs: unambiguous
  detection evidence, the literal invoke form, real `versionCmd` output, the
  pinning file, and a non-interactive install command
- A docs form covers wrong, missing, or stale pages
- The pull request template requires tests, a changelog entry, a stress-test
  update, docs when behavior changes, and an AI-assistance disclosure

#### Brand assets
- `.github/assets/` carries lockups whose wordmark is converted from `<text>` to
  outline paths, because an SVG loaded as an image cannot use a page's
  `@font-face` and every renderer without DM Sans installed was silently
  substituting a fallback face
- Glyphs are shaped with HarfBuzz against a `wght=500` instance of the DM Sans
  the site ships, then tracked (−0.056em) so the wordmark ink lands on the
  original canvas edge
- PNG copies sit alongside for renderers that reject SVG, and the Lattice &
  Company logos are keyed to transparency so they read on either GitHub theme
- The site ships the same outlined files at `public/brand/lockup-*.svg`, on the
  original viewBox, so every consumer of them is unaffected
- The composed lockups in the navbar and footer are an animated mark beside live
  `lattice` text. They track to `--wordmark-tracking`, the same −0.056em the
  outlines were built with, so the wordmark reads identically whether it is set
  as text or drawn from the asset. The mark's draw-in animation is unchanged
- The Inkscape sources under `marketing/` keep live `<text>`, since they are the
  editable originals

### Nested repos: docs, worked example, and tests — 2026-07-28

#### Docs
- A subtree that already has its own task runner is declared as a manual
  workspace whose scripts shell out to that runner; this needed no feature work,
  since ordering, `dependsOn`, caching as one opaque unit, and validation all
  fall out of the existing mechanism
- The nested-repos page covers the config, what each tool owns, and the `ignore`
  set that broad `inputs` require: dependency trees, the inner runner's own
  cache, and output directories
- Two limitations are documented. A manual workspace must declare any task
  invoked directly. A downstream workspace must not copy an upstream artifact at
  build time, because a cache key covers only the inputs its own workspace
  declares

#### Example & tests
- `examples/nested-repo` is a runnable repo with a real JS monorepo (npm
  workspaces, two packages, an inner dependency edge) as one Lattice node, plus
  a downstream service
- `e2e_passthrough.rs` and a new stress-test section prove the nested repo runs
  in graph order, provisions no toolchains, caches and restores as one unit,
  re-runs on an inner source edit, and never reports a hit when the inner
  runner's cache directory is left unignored

### Docs site search — 2026-07-28

#### Site
- Full-text search over the documentation, built on Pagefind; the index is
  generated from the built HTML as the last step of `npm run build` and ships as
  static files
- `data-pagefind-body` on the docs article scopes the index to documentation
  prose, keeping the landing page and 404 out
- Page title and section come from the collection frontmatter rather than being
  scraped out of headings
- The palette opens with `⌘K`/`Ctrl-K` or `/`, walks results with the arrow
  keys, and lists heading-level matches beneath each page so a long page points
  at the section that matched
- With no index present the palette shows an explanatory notice; this is the
  case under `astro dev`, since search needs `npm run build && npm run preview`

### Docs site deploys from CI — 2026-07-28

#### Site
- `.github/workflows/docs.yml` builds the site on every pull request touching
  `apps/web/**` and deploys to GitHub Pages on push to `mega`
- The build asserts the search index exists, so a missing index fails CI instead
  of shipping a search box that returns nothing
- Pages deploys are serialized and never cancelled mid-flight

### Docs navigation skeleton — 2026-07-28

#### Site
- The sidebar now covers the planned documentation set under Overview, Guides,
  Concepts, and Reference
- Templates, Task graph, Persistent tasks, and CLI reference join as
  placeholders; Configuration, Caching, and Toolchains move to the groups they
  belong in
- The placeholder pages are empty on purpose and their content is tracked
  separately

### lattice-docs agent — 2026-07-28

#### Repo
- A repo-scoped subagent in `.claude/agents/` that carries the architecture, CLI
  surface, config schema, and brand voice as standing context, so documentation
  work starts from the real model instead of rediscovering it
- It maps the crate graph and the five mental models: the engine gradient, the
  evidence ladder, the DAG and schedule, cache identity, and output modes. It
  also points at where each audience's docs live
- Every behavioral claim it writes has to trace to a file before it ships

### Site build output is no longer committed — 2026-07-28

#### Repo
- `apps/web/dist/` and `apps/web/.astro/` were tracked, so every local build
  dirtied the tree and the repo carried a stale copy of the site
- Both are now ignored and untracked; CI builds the site from source

### Tailwind utilities are prefixed to stop Bootstrap collisions — 2026-07-27

#### Site
- Tailwind utilities are namespaced with a `tw:` prefix, so the v4 on-demand
  scanner cannot regenerate bare classes that collide with Bootstrap
  (`collapse`, `container`, `col-*`, …)
- An unprefixed setup emitted `.collapse{visibility:collapse}` plus a Tailwind
  `.container` and grid, which hid the navbar actions and the docs sidebar and
  skewed the layout
- Write Tailwind utilities as `tw:flex`, `tw:text-teal-500`

### Dependencies upgraded across the workspace and the site — 2026-07-27

#### Rust
- Notable majors: petgraph 0.6→0.8, sha2 0.10→0.11, console 0.15→0.16, indicatif
  0.17→0.18, dialoguer 0.11→0.12, jsonschema 0.48→0.49, plus caret-compatible
  bumps via `cargo update` (anyhow, clap, tokio, serde, chrono, libc, indexmap,
  tempfile, …)
- sha2 0.11 dropped the `io::Write` impl on hashers, so the cache file digest
  now reads bytes in an explicit loop
- dialoguer 0.12 takes `Select::items` by value
- Build, tests, clippy, and the stress test all pass on the new versions

#### Site
- `apps/web` moved to Astro 7 (from 5), Tailwind CSS v4 (from v3), and the
  latest `@astrojs/*`, React, Sass, and `astro-seo` releases
- Tailwind now runs through the CSS-first `@tailwindcss/vite` plugin; the
  deprecated `@astrojs/tailwind` integration was removed
- The brand theme lives in `src/styles/tailwind.css`, and Tailwind is imported
  without preflight so Bootstrap keeps owning the reset

### Persistent tasks stream their output by default — 2026-07-27

#### Runner
- A `lattice run` that pulls a persistent task into its closure now defaults to
  raw line-by-line output instead of the live TUI, so the process's streaming
  output stays visible; this previously required `-l` (`--loquacious`)
- Non-persistent runs on a terminal still get the interactive TUI
- A persistent task's output always streams live even in raw mode, while other
  per-task output stays collapsed and is surfaced on failure
- Auto-detection no longer fabricates a command for a `persistent` task: a
  direct-invoke driver (cargo, go, …) used to invent a command for any task
  name, so `lattice run dev` picked up every Rust and Go workspace as `cargo
  dev` or `go dev` even though no such task exists
- A persistent task now runs only where the workspace declares it, through an
  explicit `scripts` entry or a manifest script for the JS and Deno drivers;
  non-persistent tasks (`build`, `test`, …) still infer as before

### Stacked commands and a self-healing editor schema — 2026-07-27

#### CLI
- `lattice run` accepts multiple tasks in one invocation (`lattice run lint test
  build`); the roots merge into a single dependency graph, so a dependency
  shared by several roots runs once and independent roots parallelize where the
  graph allows
- All existing flags (`--filter`, `--concurrency`, `--continue`, `--dry-run`,
  `--no-cache`) apply to the combined run, and an unknown task in the list fails
  fast and names the offender
- `--sequentially` / `-s` runs each task's graph to completion in the order
  given before starting the next; fail-fast stops at the first failed phase, and
  `--continue` runs the remaining phases and still exits non-zero
- `run`, `setup`, and `prune` write `.lattice/schema.json` when it is missing,
  as happens with a cleared cache directory or a clone where it was never
  committed, so an editor's JSON language server can resolve the config's
  `$schema`
- An existing copy is left untouched to avoid churn, and the schema is committed
  to this repo so validation works before the first run

### Marketing and docs site — 2026-07-27

#### Site
- A single Astro site combining the landing page and the documentation, built
  primarily in React with Bootstrap and styled to the monochrome brand system
- The hero draws the rosette mark on load, with dashed threads running in from
  each supported language (Go, Node.js, Python, Ruby, .NET, JVM)
- The landing page carries a readable `lattice.json` and terminal sample, and an
  install action
- A post-footer band places the mark half cut off at the left edge with the
  woven-arc pattern tiling to the right
- The docs shell is sidebar, content, and an on-this-page table of contents,
  driven by a Markdown/MDX content collection
- DM Sans, DM Mono, and Bootstrap Icons are self-hosted and loaded through SASS
- Light, dark, and system themes, keyboard focus, and reduced-motion support
- `apps/web` is a workspace in `lattice.json`, so `lattice run build --filter
  web` builds the site and caches the result

#### Copy
- Corrected the `lattice.json` examples on the landing page and in Getting
  started to the real schema: a `workspaces` array with `name` and `path`,
  plural `engines` version constraints, and `tasks` in place of the outdated
  `pipeline` key
- Dropped the "nothing to set up" and "no config to write" claims, which
  overstated the tool. A `lattice.json` is required, `lattice init` scaffolds
  it, and workspaces are declared explicitly
- The replacement copy states what is true: Lattice infers each workspace's
  build from its native manifest, so there are no per-language build scripts to
  write
- Hero language logos use real brand artwork through Astro's `<Image>` component
- The footer carries the Lattice & Company parent lockup, a rosette with a
  self-hosted DM Serif Text wordmark
- Copy no longer promises offline-only operation

### The documented install command installed the wrong software — 2026-07-27

#### Docs
- The docs site told readers to run `cargo install lattice`, but `lattice` on
  crates.io is an unrelated markdown linter, so anyone following the
  getting-started page or the landing-page copy button got someone else's tool
- Both now show the repo-local bootstrap one-liner, with `cargo install --git …
  lattice` documented as the from-source path

### License of record was inconsistent — 2026-07-27

#### Repo
- `LICENSE` is ISC while the workspace manifest declared `license = "MIT"`; the
  manifest now says `ISC`
