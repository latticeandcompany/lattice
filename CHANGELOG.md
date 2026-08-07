# Changelog

Notable changes, newest first. Versions follow semver: a major bump means a breaking
change to the `lattice.json` schema or the CLI surface.

A version bump is a full cache miss. The version is part of every task hash, so the
first run after an upgrade re-runs everything.

## Unreleased

<!-- Add your entry here, as a `###` section titled for what changed. -->

### A task command with a quote in it works on Windows — 2026-08-07

A task's command was handed to `cmd` as an ordinary argument. Rust quotes
arguments the way the MSVC runtime parses them, which escapes an embedded `"` as
`\"` — and `cmd` does not read `\"` as an escape, so any command containing a
quote arrived mangled. `node -e "console.log(1)"` was enough, as was any path
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
— a nested repo's task runner and a published release binary — are small Rust
programs rather than shell scripts, so they run wherever they were built.

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
  own outright — `.lattice`, say — that took `toolchains/` and `bin/` with it,
  including the installed binary. Only directories whose names have the shape of
  a cache format are reclaimed now
- Ctrl-C left every running task's children alive. Each task runs in its own
  process group, which is what lets a task that shells out be cleaned up as a
  unit, and the same call detaches it from the terminal's Ctrl-C — so the signal
  reached Lattice, Lattice exited, and the compilers kept going. An interrupt now
  sends `SIGTERM` to every running group, waits up to five seconds, then kills
  what is left, and exits `130` rather than `1`. A cancelled CI job is not a build
  that broke
- A `dependsOn` that named nothing was a silent no-op. A workspace depending on a
  misspelled workspace name, or a task depending on a task the `tasks` map never
  defined, built no edge — so the ordering the config was written to guarantee
  simply did not happen, with nothing printed. Both are now rejected at load,
  with the nearest name offered
- A workspace `path` could be absolute or climb out of the repo with `..`. The
  workspace directory bounds which files are hashed, which the `outputs` globs
  match, and which a cache hit clears before unpacking, so a path that left the
  repo put all three somewhere Lattice has no business writing. Rejected at load
- `settings.maxCacheSize` was inert. It read as a budget but only `lattice prune`
  consulted it, so a repo that set one still grew without limit. Every run now
  holds the cache to it. With no budget set the cache still grows without limit,
  which is why there is no default
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
  what moved. It is now built from named components — `inputs`, `env`,
  `globalEnv`, `globalDependencies`, `dependencies`, `manifests`, `toolchain`,
  `command`, `patterns`, `environment` — and each is recorded per workspace and
  task, so `-l` reports `cache miss: inputs changed` instead of a bare miss

### The cache key covers what actually determines a task's result — 2026-08-03

The first run after this upgrade re-runs everything. Entries are now grouped by
cache format on disk, and the previous group is retired by the next
`lattice prune` rather than read.

- Two workspaces could share one cache entry and restore each other's artifacts.
  The key did not include the workspace, so a task running the same command in
  two places with nothing else to distinguish it resolved to one identity — the
  second workspace reported a hit and unpacked the first one's build. The
  workspace name is now part of every key
- A change in a dependency did not reach the tasks that depend on it. `dependsOn`
  decided the order and nothing else, so editing a library rebuilt the library
  and then served its consumers from cache — against code that no longer existed.
  Every task's key now includes the resolved keys of its prerequisites. This is
  conservative by design: a workspace is one node, so a dependent re-runs when
  its dependency changes even if the specific files it reads did not
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
  hoists one to the top — pnpm, yarn and npm workspaces, a Cargo virtual
  workspace — got no invalidation at all from a dependency bump. The repo root is
  now checked too
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
  It used to store an empty archive, which verified perfectly forever after — so
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
suspect entry — which is what it was always reached for, and what it could not
do while it also declined to write.

### `lattice init` reads the repo instead of asking about it — 2026-07-31

- `init` opened with "what does this repo need Lattice for — build tool,
  toolchain manager, or both?", then had you type every workspace name and path
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
  is named on the way out. Declaring it would halt the very next run, so `init`
  proposes a config that works and tells you what it held back and why

### A persistent task that exits is reported — 2026-07-30

- Lattice spawned a dev server and never looked at it again. A `persistent: true`
  task whose command exited — a port already taken, or a one-shot command marked
  persistent by mistake — left the run reporting it as running until Ctrl-C, then
  printing `0 failed`. Every persistent child is now waited on
- An exit that isn't a clean `0` prints
  `web:dev: EXITED (code 1) after 1.09s` on stderr, counts in the run summary's
  failed count, and exits non-zero. A signal reads `EXITED (killed by signal)`
- An exit code of `0` prints the same line lowercased on stdout and counts as
  nothing. The process you asked to keep running is gone either way, so the run
  says so
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
- **This breaks any `lattice.json` carrying an extra key** — a leftover
  `projects` map, a `settings.logging` from before it was removed, a `glob` on a
  workspace entry, a note parked under a key of your own. Delete it; there is no
  opt-out
- The reason it is worth breaking: `output` for `outputs` left a task declaring
  nothing to capture, so a cache hit restored no files, and `input` for `inputs`
  hashed no files, so the task hit the cache after its first run whatever you
  edited. Both were silent
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

### A cache hit restores files, and now says so — 2026-07-30

- `CacheStore::restore`'s rustdoc claimed the caller re-exports the entry's
  stored `env`. Nothing ever did, and nothing can: a hit starts no process, so
  there is no environment to export into. The runner's matching dead read — an
  `entry.env()` assigned to `_cached_env` under a comment that made it look
  deliberate — is gone
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
- Where a candidate tool does exist, nothing changes — every tool a generic
  ecosystem marker maps to can drive, so a bare `package.json` still suggests
  `pnpm`. A test asserts that stays true as the marker table grows
- The stress test now pastes the suggested fix back in and asserts the run
  succeeds

### The docs say what happens instead of arguing for it — 2026-07-30

- A voice pass over all 28 pages of `apps/web/src/content/docs/`. Removed the
  framing sentences that announced what a section was about to say, the
  commentary on our own choices (`and that is deliberate`, `by design`,
  `is the point`), and the bolded-lead-in bullet list used as a default
  structure. Prose where it reads better, tables where the content is a table
- `environment-variables.md` no longer tells you to prefer flags over variables.
  Both are supported, the flag wins where both are given, and the page says so
  once instead of organizing itself around it. The column headed
  `Flag to prefer` is now `Flag`, and the section justifying the preference is
  replaced by one explaining the version-switch handover — the actual reason
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

### The multi-language example now orders something — 2026-07-30

- `examples/polyglot` declared `"build": { "dependsOn": ["^build"] }` while no
  workspace in it declared `dependsOn`. `^build` expanded to nothing, all four
  workspaces were independent roots, and the example demonstrated none of the
  cross-language ordering it exists to show. `worker` (Go) now declares
  `"dependsOn": ["utils"]` (Python), so `utils:build` runs first
- A test asserts the pair holds: if the example keeps a `^task` but loses every
  workspace edge, `cargo test -p lattice-config` fails
- The walkthrough at `apps/web/src/content/docs/multi-language-monorepo.md` no
  longer demonstrates the edge as a hypothetical addition — the run output on
  that page is recaptured against the example as it now ships, as is the quick
  start in the README

### `settings.logging` is gone — 2026-07-30

- The field validated against the bundled schema and changed nothing. Nothing in
  the tree read it. It is removed from the config type and from the schema
- Output verbosity is `-l`/`--loquacious`, `settings.loquacious`, and `CI`, which
  is what it always was
- A `lattice.json` still carrying `logging` keeps loading — unknown settings are
  ignored, not rejected — so nothing breaks on upgrade. Editors pointed at the
  refreshed `.lattice/schema.json` will start flagging the key. Delete it
- `.lattice/schema.json` is only written when absent, so a repo initialized
  before this keeps its copy. Delete the file and run any `lattice` command to
  pick up the current one

### Flags for what used to be environment variables — 2026-07-29

- `--theme light|dark` replaces `LATTICE_THEME`, and picks the teal shade of the
  splash art. A value that is neither is now a parse error listing the two that
  work, rather than a silently ignored string. It is global, so it parses on
  `lattice` itself and on every subcommand
- `--release-base-url <URL>` replaces `LATTICE_RELEASE_BASE_URL`. Also global,
  because `upgrade` is not the only thing that downloads — an invocation in a
  repo pinning a version that is not installed fetches it under whatever command
  you typed
- `--release-latest-url <URL>` and `--release-list-url <URL>` replace
  `LATTICE_RELEASE_LATEST_URL` and `LATTICE_RELEASE_LIST_URL`. These sit on
  `lattice upgrade`, the only command that resolves `latest`
- Every one of those variables still works. The flag wins where both are given,
  and a blank value at either step falls through to the default rather than
  building an empty URL
- `LATTICE_SWITCHED_FROM` stays a variable on purpose. It is read by the process
  a version switch hands the invocation to, and that process is a different
  build of Lattice — an older one would reject a flag it has never heard of and
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
  color a task gets can differ between runs — within a run it never changes
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
  workspace is unchanged — `deno` still drives as a task runner, `bun` as a
  package manager — but a dual-role tool no longer reads as a lie in the table
- The well-known engine list and the built-in version commands were two separate
  tables that disagreed. `uv`, `poetry`, `just`, `turbo`, `nx`, `swift`, `dart`,
  `composer`, `mix`, `stack`, `cabal`, `pdm`, and `pipenv` all had a version rule
  but were rejected in string form, so `"engines": { "uv": ">=0.5" }` failed to
  load for no reason. They are one table now, in `lattice-config`, and every
  driver is guaranteed a row in it
- `"python3": ">=3.12"` was checked by running `python --version`, which on many
  machines is a different interpreter — or Python 2. It runs `python3 --version`
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
  pip, uv, and pip-tools alike, and a Kotlin project is driven by gradle or
  maven — so `pip` and `kotlin` are selected by declaration, never by guessing.
  For the same reason `packages.lock.json` is not nuget evidence: an SDK-style
  project can carry one and still be a `dotnet` workspace

### An agent skill, so a coding agent stops guessing at lattice.json — 2026-07-29

- New `skills/lattice/`: a `SKILL.md` plus four references — the `lattice.json`
  field reference, the CLI surface, the driver and engine tables, and
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

### Links that go where they say, and a Get started page — 2026-07-29

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

### Dependency bumps, and auto-merge that can actually merge — 2026-07-28

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
  `latticeandcompany.github.io/lattice/install.sh` with no build step of its own,
  and the release workflow publishes that same file as a release asset. One copy,
  two homes
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
- A binary Lattice did not install — `cargo install`, a distro package,
  `scripts/dev-link.sh` — is never replaced. Those keep the advisory one-line nag
  from #45, which now prints a runnable `lattice upgrade <version>`
- `--no-version-check`, `LATTICE_NO_VERSION_CHECK` and
  `settings.versionCheck: false` each skip the whole thing. `upgrade`, `version`
  and `completions` are never handed off: they answer for the binary that was
  invoked, and a completion script has to be the only thing on stdout
- A pinned version that cannot be installed is a hard failure naming the version
  and the way past it. Running a build the repo did not ask for is the outcome
  this exists to prevent

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
- `.github/workflows/release.yml` builds `v*` tags for six targets — macOS
  x86_64/aarch64, Linux x86_64 gnu and musl, Linux aarch64, Windows x86_64 — and
  publishes `lattice-<version>-<target>.tar.gz` archives carrying the binary, the
  license and completion scripts, plus one `lattice-<version>-checksums.txt` and
  the installer
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

- `marketing/BRAND.md` still shipped `https://github.com/<org>` in the donated-project
  endorsement footer, in both the prose form and the copy-paste HTML. Any project that
  followed the spec would have published a dead link. All three now point at
  `https://github.com/latticeandcompany`, matching the `repository` in `Cargo.toml`

### Four documented promises the code did not keep — 2026-07-28

Groundwork for the first tagged release. Each item here was a statement in the README,
the docs or a manifest that the code contradicted.

#### The stated minimum Rust version was wrong by eleven minor versions
- `Cargo.toml` declared no `rust-version` at all, while the README badge, the README
  Development section, `CONTRIBUTING.md` and `lattice.json`'s `engines.cargo` all claimed
  1.75. Resolving the lockfile against each dependency's own `rust-version`, the real floor
  is 1.86: `clap`, `sha2` and `indexmap` need 1.85, and the ICU crates reached through
  `jsonschema` → `idna` → `idna_adapter` need 1.86. A build on 1.75 could not have worked
- `rust-version = "1.86"` is now set once in `[workspace.package]` and inherited by all
  seven crates, and the four prose copies are corrected to match. `cargo +1.86 check
  --workspace --all-targets --locked` passes
- Declaring it turned on clippy's `incompatible_msrv` lint, which found one real
  violation: `u64::is_multiple_of` in `lattice-config` is stable since 1.87. It is now
  `bytes % unit == 0`, which is what the method is sugar for, so the floor stays where the
  dependency tree actually puts it
- `jsonschema` is a dev-dependency taken with `default-features = false`. Its default
  features pull `reqwest` in for remote `$ref` resolution, which the schema tests never
  use — that dragged a whole TLS stack into the dev tree, against `CONTRIBUTING.md`'s rule
  about network access. All 23 schema tests pass without it

#### `version --json` could not report a target triple
- The `target` field was `std::env::consts::ARCH`, so it printed `aarch64` rather than
  `aarch64-apple-darwin`. `bug_report.yml` asks contributors for that output to identify a
  platform, and the installer needs the same vocabulary to pick a release asset
- A `build.rs` on `crates/lattice` now emits `LATTICE_TARGET` from cargo's `TARGET`, and
  the bare architecture moves to its own `arch` field rather than being dropped
- The stress test asserted only that a `target` key existed, which a bare arch satisfied.
  It now also asserts the value looks like a triple, and a missing `version` field is a
  hard failure instead of silently falling back to a hardcoded `0.1.0`

#### A docs page described a command that does not exist
- `apps/web/src/content/docs/templates.md` documented scaffolding new workspaces from a
  template. There is no `templates` command in `crates/lattice/src/commands/`, and
  `CONTRIBUTING.md` prohibits documenting features that do not ship. The page is deleted
  and `nested-repos.md` moves up to fill the gap it left in the Guides ordering

#### Windows was presented as merely untested
- `lattice-runner` has a correct `cmd /C` branch, but `lattice-workspace`'s toolchain
  probe hardcodes `sh -c` and joins `PATH` with `:`, so engine version checks and toolchain
  provisioning — half of what the product claims to do — cannot work there. A Windows
  binary would launch, print a splash, and fail at the detection ladder
- The README and the getting-started page now say macOS and Linux, and point Windows users
  at WSL2

#### Manifest metadata
- `[workspace.package]` gains `repository`, `homepage`, `documentation`, `keywords` and
  `categories`, none of which any crate carried, plus `publish = false` so that a stray
  `cargo publish` cannot push. Five of the seven crate names are unclaimed on crates.io, so
  without that guard a mistyped `-p` would permanently publish an unsupported library;
  `lattice` and `dagger` are both taken by unrelated projects
- `[profile.release]` sets `strip`, thin LTO and one codegen unit, since release output is
  now something we ship rather than something we only build locally

### Repo hygiene pass — 2026-07-28

#### Line endings, and the binaries they destroyed
- `.gitattributes` was a single `* text eol=crlf` rule. Because `text` was *set* rather than auto-detected, git applied CRLF normalization to binary files as well. A PNG's 8-byte signature contains a `CR LF` pair, so normalizing one corrupts it: `.github/assets/latticeco-black.png`, `latticeco-white.png` and `apps/web/src/assets/languages/node-dark.png` were all committed damaged. The two company logos were unreadable in the README
- The rule also gave every shell script a CRLF shebang, which makes it unrunnable (`env: bash\r: No such file or directory`). `scripts/dev-link.sh`, `scripts/dev-unlink.sh`, `scripts/stress-test.sh` and `examples/nested-repo/services/api/src/serve.sh` could not execute, and the README documents two of them as the way to work on the repo
- `.gitattributes` now lists authored text extensions explicitly, holds `.sh` and `.txt` at LF, and marks binary formats `binary` so no future `text` rule can reach them. `rosette.txt` is included with `include_str!` and printed to a terminal, which is why it is LF
- `node-dark.png` is repaired from its intact working-tree copy. The two `latticeco-*.png` logos were damaged in both the blob and the working tree, with no earlier commit to recover from, so they are re-exported from `marketing/lattice-and-co/lockup-horizontal-*.png` — now 480px wide with an alpha channel, so the dark-theme copy no longer renders as a black box on GitHub

#### Rust formatting
- Added `rustfmt.toml` with `hard_tabs = true`. Rust was 4-space while `.editorconfig` and `CODESTYLE.md` both call for tabs; 21 files are reformatted. Every other rustfmt setting stays default

#### CODESTYLE.md
- The file was ArenaSwap's copied verbatim: it mandated camelCase filenames and single quotes repo-wide, listed `wxt.config.ts` and `turbo.json` as protected filenames (neither exists here), and gave no Rust guidance at all in a repo that is mostly Rust
- It now splits into a Rust section (`crates/`) and a Web section (`apps/web/`), with the shared principles kept common. The Rust section covers rustfmt and clippy as gates, naming, crate layout, `anyhow` error context, and the `//!`/`///` rules that `AGENTS.md` already implies. `dagger` is recorded as the one deliberate exception to `lattice-<role>` crate naming, and PascalCase Astro layouts are recorded as the exception to camelCase filenames

#### Ignored and untracked
- `.gitignore` now covers `.DS_Store`, `.idea/`, `.vscode/` and `*.log`. Three `.DS_Store` files were sitting in the repo, kept out only by a machine-local global excludes file
- `dist/` and `.astro/` are no longer scoped to `apps/web`; running the site from the repo root drops a `.astro/` at the root instead
- Running the examples dirtied the repo, which CONTRIBUTING tells contributors to do. `examples/polyglot/apps/web/dist/index.html` and `examples/polyglot/libs/utils/dist/utils.py` were committed build outputs and are now untracked; the ignore list covers every example's `dist/`, `target/`, `.venv/`, `bin/` and `__pycache__/`

#### GitHub
- Added `.github/dependabot.yml` covering the Cargo workspace, the docs site's `apps/web` lockfile, and GitHub Actions, on the same Friday cadence and `Upgrade` commit prefix the other repos use
- Added `.github/workflows/dependabot-automerge.yaml`. It listens on `pull_request` only — the ArenaSwap original also listens on `pull_request_target`, which runs every step twice and hands `contents: write` to a fork-triggered workflow. Patches auto-merge; minors auto-merge for the docs stack and the crates the test suite exercises, while anything touching caching, hashing or process control waits for review
- Added `.github/copilot-instructions.md` pointing at `AGENTS.md`
- Deleted `.github/assets/lockup-black.png` and `lockup-white.png`, which nothing referenced — the README uses the SVGs
- README gains the CI, stars, forks, issues and last-commit badges the other repos carry

#### marketing/
- Filenames are kebab-case throughout: `lattice_icon_black.svg` → `icon-black.svg`, `favicon_black.svg` → `favicon-black.svg`, `ascii-art_full.txt` → `ascii-art-full.txt`, and the `_lockup` variants → `lockup-black.svg` / `lockup-white.svg`
- `Lattice & Co/` had a space and an ampersand in its path, mode 700, and six files named `1.png` through `6.png`. It is now `lattice-and-co/` at mode 755 with the marks named for what they are — `lockup-horizontal-*`, `lockup-stacked-*`, `monogram-*`
- `BRAND.md`'s asset table is updated for the new names and gains the rows it was missing (`pattern.svg`, both ascii-art files, the company marks). A new "Where each copy lives" section explains why the same lockup exists in `marketing/`, `apps/web/public/brand/` and `.github/assets/`: the first has a live `<text>` wordmark and is the editable source, the other two are outlined so they render without DM Sans installed

#### Stale copy
- Root `lattice.json` listed `tailwind.config.cjs` as a build input. The file does not exist; the docs site is Tailwind v4 through the Vite plugin
- `examples/polyglot/apps/web/package.json` described itself as a Next.js app. It is an `echo` and `mkdir` script

### Long durations print as a clock — 2026-07-28

- Task and run times over a minute now read `4:07` and `1:12:30` instead of `247.00s` and `4350.00s`. Under a minute is unchanged (`1.23s`). Applies everywhere `lattice` prints a duration: per-task completion lines and the run summary, in both the interactive and CI reporters

### Copy and comment cleanup across the repo — 2026-07-28

#### Voice guides
- `marketing/BRAND.md` §4 is now an enforceable contract rather than three bullets: it names the banned words, the pillar phrasings that must not ship, and the specific slop patterns that kept reappearing — bolded-lead-in bullet lists, rhythmic triads, unmeasurable comparatives, invented numbers, em-dash dramatic pauses, and aphorisms about ourselves
- The voice table previously used the banned word "polyglot" in its own recommended column, and offered invented benchmarks ("Cold builds in 4s. Cached in 90ms.") as the model answer in a section requiring measurable claims; both are corrected
- `marketing/MESSAGING.md` value props were named after the four core pillars, which is what the guidelines forbid; they are renamed for what the user gets
- The official tagline is recorded as an explicit, documented exception to the performance-adjective and pillar rules, so a future pass does not "correct" it
- `.claude/agents/lattice-docs.md` prescribed `### Added/Changed/Fixed` with bold lead-in entries, which was generating the changelog style it produced; it now carries the current format

#### Copy
- The tagline is one string in 12 places, including the `crates.io` package description in `crates/lattice/Cargo.toml`. Two competing variants were in use
- "polyglot" is gone from all prose; the `examples/polyglot/` path is unchanged
- Every `https://lattice.build` URL is replaced. The domain is not registered, so the documented install command and all five docs links were dead
- Installation now documents the from-source path that works. The advertised `curl | sh` one-liner had no installer behind it at any URL, and there is no release to fetch a binary from
- GitHub URLs are normalized to the `latticeandcompany` org; the repo previously used two owners across 12 links

#### Crates
- 67 dash-padded banner comments removed across 8 files, including the 22 `// ---- x tests ----` markers inside modules already named `mod tests`
- Every `(PRD §…)`, `(BRAND.md §…)`, and `(decision #…)` citation is gone. The PRD was deleted in `120ad78`, so all 19 were dangling references to a document that does not exist
- Redundant doc comments that restated the item's name are deleted, along with comments narrating past fixes or congratulating the code
- All-caps prose emphasis, rhythmic triads, and pillar bragging removed from module headers and doc comments
- User-facing error messages follow one convention: lowercase start, no trailing period
- Four stress-test assertions and four unit-test assertions updated to the new message casing

#### Docs
- The orphaned 228-line `docs/toolchains.md` is folded into the published `apps/web/src/content/docs/toolchains.md`, which was a 14-line stub pointing back at the repo file, and the root duplicate is deleted
- `docs/` no longer exists; the published docs are the single source

### Public repository documentation — 2026-07-28

#### Repo
- `.github/README.md` leads with the lockup and the tagline, the install one-liner, and a quick start whose terminal output is captured from a real run of `examples/polyglot` — a cold run, then the same run entirely from cache
- `CONTRIBUTING.md` covers the crate layout, the `feature/* → mega` flow, the build and test commands, dogfooding through `scripts/dev-link.sh`, and the Rust standards: clippy clean at `-D warnings`, no `unwrap`/`panic!` on user-reachable paths, errors propagate
- The testing policy requires tests with every change, the stress test updated and exiting `0`, and cache work proved in both directions
- The AI-assistance policy accepts agent-written contributions on the condition that they are disclosed and that the human author owns every line
- `CODE_OF_CONDUCT.md` sets out the enforcement ladder, the prohibited-conduct list, and appeals routed to the maintainer
- `SECURITY.md` routes reports through private GitHub advisories and states the threat model: executing declared commands, running detected tools, and provisioning a declared `installCmd` are intended behavior, while cache poisoning, extraction path traversal, escapes from `./.lattice/`, distribution integrity failures, and leaked secrets are in scope

#### Issue and pull request templates
- Four YAML forms in `.github/ISSUE_TEMPLATE/`; blank issues are off and questions route to Discussions
- Bug reports collect `lattice version --json`, the platform, the relevant `lattice.json`, the exact command, and `-l` output
- Feature requests ask whether the thing is already possible by declaring the commands yourself, and gate on not requiring Lattice to identify a tool it has no evidence for
- The toolchain form asks for what a new driver actually needs: unambiguous detection evidence, the literal invoke form, real `versionCmd` output, the pinning file, and a non-interactive install command
- A docs form covers wrong, missing, or stale pages
- The pull request template requires tests, a changelog entry, a stress-test update, docs when behavior changes, and an AI-assistance disclosure

#### Brand assets
- `.github/assets/` carries lockups whose wordmark is converted from `<text>` to outline paths, because an SVG loaded as an image cannot use a page's `@font-face` and every renderer without DM Sans installed was silently substituting a fallback face
- Glyphs are shaped with HarfBuzz against a `wght=500` instance of the DM Sans the site ships, then tracked (−0.056em) so the wordmark ink lands on the original canvas edge
- PNG copies sit alongside for renderers that reject SVG, and the Lattice & Company logos are keyed to transparency so they read on either GitHub theme
- The site ships the same outlined files at `public/brand/lockup-*.svg`, on the original viewBox, so every consumer of them is unaffected
- The composed lockups in the navbar and footer — an animated mark beside live `lattice` text — track to `--wordmark-tracking`, the same −0.056em the outlines were built with, so the wordmark reads identically whether it is set as text or drawn from the asset. The mark's draw-in animation is unchanged
- The Inkscape sources under `marketing/` keep live `<text>`, since they are the editable originals

### Nested repos: docs, worked example, and tests — 2026-07-28

#### Docs
- A subtree that already has its own task runner is declared as a manual workspace whose scripts shell out to that runner; this needed no feature work, since ordering, `dependsOn`, caching as one opaque unit, and validation all fall out of the existing mechanism
- The nested-repos page covers the config, what each tool owns, and the `ignore` set that broad `inputs` require: dependency trees, the inner runner's own cache, and output directories
- Two limitations are documented — a manual workspace must declare any task invoked directly, and a downstream workspace must not copy an upstream artifact at build time, because a cache key covers only the inputs its own workspace declares

#### Example & tests
- `examples/nested-repo` is a runnable repo with a real JS monorepo (npm workspaces, two packages, an inner dependency edge) as one Lattice node, plus a downstream service
- `e2e_passthrough.rs` and a new stress-test section prove the nested repo runs in graph order, provisions no toolchains, caches and restores as one unit, re-runs on an inner source edit, and never reports a hit when the inner runner's cache directory is left unignored

### Docs site search — 2026-07-28

#### Site
- Full-text search over the documentation, built on Pagefind; the index is generated from the built HTML as the last step of `npm run build` and ships as static files
- `data-pagefind-body` on the docs article scopes the index to documentation prose, keeping the landing page and 404 out
- Page title and section come from the collection frontmatter rather than being scraped out of headings
- The palette opens with `⌘K`/`Ctrl-K` or `/`, walks results with the arrow keys, and lists heading-level matches beneath each page so a long page points at the section that matched
- With no index present the palette shows an explanatory notice; this is the case under `astro dev`, since search needs `npm run build && npm run preview`

### Docs site deploys from CI — 2026-07-28

#### Site
- `.github/workflows/docs.yml` builds the site on every pull request touching `apps/web/**` and deploys to GitHub Pages on push to `mega`
- The build asserts the search index exists, so a missing index fails CI instead of shipping a search box that returns nothing
- Pages deploys are serialized and never cancelled mid-flight

### Docs navigation skeleton — 2026-07-28

#### Site
- The sidebar now covers the planned documentation set under Overview, Guides, Concepts, and Reference
- Templates, Task graph, Persistent tasks, and CLI reference join as placeholders; Configuration, Caching, and Toolchains move to the groups they belong in
- The placeholder pages are empty on purpose and their content is tracked separately

### lattice-docs agent — 2026-07-28

#### Repo
- A project subagent in `.claude/agents/` that carries the architecture, CLI surface, config schema, and brand voice as standing context, so documentation work starts from the real model instead of rediscovering it
- It maps the crate graph and the five mental models — engine gradient, evidence ladder, DAG and schedule, cache identity, output modes — and points at where each audience's docs live
- Every behavioral claim it writes has to trace to a file before it ships

### Site build output is no longer committed — 2026-07-28

#### Repo
- `apps/web/dist/` and `apps/web/.astro/` were tracked, so every local build dirtied the tree and the repo carried a stale copy of the site
- Both are now ignored and untracked; CI builds the site from source

### Tailwind utilities are prefixed to stop Bootstrap collisions — 2026-07-27

#### Site
- Tailwind utilities are namespaced with a `tw:` prefix, so the v4 on-demand scanner cannot regenerate bare classes that collide with Bootstrap (`collapse`, `container`, `col-*`, …)
- An unprefixed setup emitted `.collapse{visibility:collapse}` plus a Tailwind `.container` and grid, which hid the navbar actions and the docs sidebar and skewed the layout
- Write Tailwind utilities as `tw:flex`, `tw:text-teal-500`

### Dependencies upgraded across the workspace and the site — 2026-07-27

#### Rust
- Notable majors: petgraph 0.6→0.8, sha2 0.10→0.11, console 0.15→0.16, indicatif 0.17→0.18, dialoguer 0.11→0.12, jsonschema 0.48→0.49, plus caret-compatible bumps via `cargo update` (anyhow, clap, tokio, serde, chrono, libc, indexmap, tempfile, …)
- sha2 0.11 dropped the `io::Write` impl on hashers, so the cache file digest now reads bytes in an explicit loop
- dialoguer 0.12 takes `Select::items` by value
- Build, tests, clippy, and the stress test all pass on the new versions

#### Site
- `apps/web` moved to Astro 7 (from 5), Tailwind CSS v4 (from v3), and the latest `@astrojs/*`, React, Sass, and `astro-seo` releases
- Tailwind now runs through the CSS-first `@tailwindcss/vite` plugin; the deprecated `@astrojs/tailwind` integration was removed
- The brand theme lives in `src/styles/tailwind.css`, and Tailwind is imported without preflight so Bootstrap keeps owning the reset

### Persistent tasks stream their output by default — 2026-07-27

#### Runner
- A `lattice run` that pulls in a persistent task — a dev server, a watcher, or anything in its dependency closure — now defaults to raw line-by-line output instead of the live TUI, so the process's streaming output stays visible; this previously required `-l` (`--loquacious`)
- Non-persistent runs on a terminal still get the interactive TUI
- A persistent task's output always streams live even in raw mode, while other per-task output stays collapsed and is surfaced on failure
- Auto-detection no longer fabricates a command for a `persistent` task: a direct-invoke driver (cargo, go, …) used to invent a command for any task name, so `lattice run dev` picked up every Rust and Go workspace as `cargo dev` or `go dev` even though no such task exists
- A persistent task now runs only where the workspace declares it, through an explicit `scripts` entry or a manifest script for the JS and Deno drivers; non-persistent tasks (`build`, `test`, …) still infer as before

### Stacked commands and a self-healing editor schema — 2026-07-27

#### CLI
- `lattice run` accepts multiple tasks in one invocation (`lattice run lint test build`); the roots merge into a single dependency graph, so a dependency shared by several roots runs once and independent roots parallelize where the graph allows
- All existing flags (`--filter`, `--concurrency`, `--continue`, `--dry-run`, `--no-cache`) apply to the combined run, and an unknown task in the list fails fast and names the offender
- `--sequentially` / `-s` runs each task's graph to completion in the order given before starting the next; fail-fast stops at the first failed phase, and `--continue` runs the remaining phases and still exits non-zero
- `run`, `setup`, and `prune` write `.lattice/schema.json` when it is missing, as happens with a cleared cache directory or a clone where it was never committed, so an editor's JSON language server can resolve the config's `$schema`
- An existing copy is left untouched to avoid churn, and the schema is committed to this repo so validation works before the first run

### Marketing and docs site — 2026-07-27

#### Site
- A single Astro site combining the landing page and the documentation, built primarily in React with Bootstrap and styled to the monochrome brand system
- The hero draws the rosette mark on load, with dashed threads running in from each supported language (Go, Node.js, Python, Ruby, .NET, JVM)
- The landing page carries a readable `lattice.json` and terminal sample, and an install action
- A post-footer band places the mark half cut off at the left edge with the woven-arc pattern tiling to the right
- The docs shell is sidebar, content, and an on-this-page table of contents, driven by a Markdown/MDX content collection
- DM Sans, DM Mono, and Bootstrap Icons are self-hosted and loaded through SASS
- Light, dark, and system themes, keyboard focus, and reduced-motion support
- `apps/web` is a workspace in `lattice.json`, so `lattice run build --filter web` builds the site and caches the result

#### Copy
- Corrected the `lattice.json` examples on the landing page and in Getting started to the real schema: a `workspaces` array with `name` and `path`, plural `engines` version constraints, and `tasks` in place of the outdated `pipeline` key
- Dropped the "nothing to set up / no config to write" claims, which overstated the tool — a `lattice.json` is required and `lattice init` scaffolds it, and workspaces are declared explicitly
- Reframed that claim to what is true: Lattice infers each project's build from its native manifest, so there are no per-language build scripts to write
- Hero language logos use real brand artwork through Astro's `<Image>` component
- The footer carries the Lattice & Company parent lockup, a rosette with a self-hosted DM Serif Text wordmark
- Copy no longer promises offline-only operation, leaving room for Lattice Cloud

### The documented install command installed the wrong software — 2026-07-27

#### Docs
- The docs site told readers to run `cargo install lattice`, but `lattice` on crates.io is an unrelated markdown linter, so anyone following the getting-started page or the landing-page copy button got someone else's tool
- Both now show the repo-local bootstrap one-liner, with `cargo install --git … lattice` documented as the from-source path

### License of record was inconsistent — 2026-07-27

#### Repo
- `LICENSE` is ISC while the workspace manifest declared `license = "MIT"`; the manifest now says `ISC`
