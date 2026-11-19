---
title: The desktop app
description: Open a project in Lattice Desktop to run tasks, read the graph, check toolchains, manage the cache, and edit the config.
group: Guides
order: 8
---

# The desktop app

Lattice Desktop runs the tasks `lattice run` runs, and shows what a terminal
cannot hold on screen at once: every workspace and what it resolved to, the
shape of the graph a task will run, which tool versions each workspace is using,
what the cache holds, and which part of a cache key moved when a task missed.

The engine is linked into the app, so the window and the CLI share one
scheduler, one cache, and one set of discovery rules. There is nothing the two
can disagree about.

## Install it

Every release publishes a bundle for each platform. The
[Lattice Desktop page](/lattice/desktop) lists them: a `.dmg` for Apple Silicon
and Intel Macs, a `-setup.exe` installer and an `.msi` for Windows on x86_64,
and a `.deb`, an `.rpm` and an AppImage for Linux on x86_64 and aarch64.

The bundles are not code-signed, so macOS and Windows both stop the first launch
and ask you to confirm. The download page names the warning each one shows, says
what to click, and publishes a checksum for every file.

## Build and run it from a clone

```sh
cd apps/desktop
npm install
npm run app
```

That starts the frontend dev server, compiles the Rust backend, and opens the
window. `npm run bundle` produces an installer for your platform, matching the
release build. Either needs the platform's webview toolchain; on Debian or
Ubuntu:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

## Open a project

A project is one repo with a `lattice.json` at its root. Open a directory from
the project control at the top of the sidebar; Lattice walks up from what you
picked, the way every command does, so picking a subdirectory works. The same
control lists every project you have opened.

**A repo with no config** opens a setup walkthrough instead of failing. It shows
the workspaces the repo's own workspace files already name, offers the projects
none of them name as extra `workspaces` entries, and offers the tool versions
the repo already pins as `engines`. It previews the exact `lattice.json` it is
about to write, which is the file `lattice init` would write, and redraws it as
you change a selection.

**A 1.x config** opens a migration view. It shows the current file beside the
one `lattice migrate` would write, what changed that the diff does not show —
an engine dropped because it only chose a tool, an edge dropped because a
manifest already states it — and writes it when you confirm. See [Upgrading to
2.0](/lattice/docs/upgrading#upgrading-to-20).

**A config that does not load** opens a screen with the error in the parser's
own words. Fix the file in your editor and press **Reload**, which opens that
config again.

Close a project, or switch to another, and the window stops whatever that
project was running. A reload of the window reopens the project and picks up a
run still going in the backend, with its output redrawn.

## Run tasks

The **Tasks** view lists every workspace with one row per task it can run —
whether it came from the workspace's manifest, its tool's verbs, or an override,
with the source shown — so it answers "what can I run here" without opening a
manifest. A workspace with no `lint` has no `lint` row, as on the command line.

Pick one or more tasks to run, the way `lattice run lint test build` stacks
them. The run controls map onto `lattice run` flags:

| Control | Flag |
| --- | --- |
| Cache: use it, force, or skip it | none, `--force`, `--no-cache` |
| Workspace filter | `-f`/`--filter` |
| Concurrency | `--concurrency` |
| Arguments | after `--`, passed to the tasks you picked |
| Keep going after a failure | `--continue` |
| Finish each task before starting the next | `-s`/`--sequentially` |

A row's own run button scopes the run to that workspace and what it depends on.
**Stop** ends a run the way Ctrl-C does: running children are terminated, and
the run reports as interrupted rather than failed.

During a run, each running task shows the last lines it printed, a queued one
names the prerequisites it is still waiting on, and a failed task opens its
output on its own with the same detail the CLI's `FAILED` line carries. A task
that missed the cache says which parts of its key moved — `inputs`, `command`,
`dependencies` and so on; see [Cache internals](/lattice/docs/cache-internals).
When the run ends, it carries the same summary the CLI prints, saved time
included, and marks a run that came entirely from cache.

## Read the graph

The **Graph** view draws the graph for the selected tasks, left to right in
dependency order. Focus a task to light everything it depends on and everything
that depends on it, which is the slice of the repo a change there can reach.
Search narrows it by name. While a run is going the graph fills in live. The
encoding is not all color: filled means ran, faded is a cache hit, a heavy
border is running or failed, a dashed outline came along as a prerequisite, and
a rounded square is persistent. A dashed edge stays inside one workspace, and a
heavier edge is a dependency written in `lattice.json` rather than read from a
manifest. The legend names the states, and a dashed node's tooltip says it is
a dependency of a task you selected. A list mode shows the same tasks as a
table a screen reader and a keyboard can walk.

## Check toolchains

The **Toolchains** view checks every workspace's engines the way a run does
before it starts, installing any whose `installCmd` has not run yet. Workspaces
that share the same engines are grouped. Each engine shows what the config wants,
the version found, where it was found — the host tool's path, or the directory
Lattice installed into — and whether Lattice installed it. A check that fails
shows the same wanted, found, ran and fix lines the CLI prints. See [Engines and
provisioning](/lattice/docs/engines).

## Manage the cache

The **Cache** view shows what `lattice stats` reports — the cache directory, its
size and entry count against `settings.maxCacheSize`, and the task time it has
saved, in total and over the last seven days — and prunes it, down to
`settings.maxCacheSize` when one is set or to empty, the way `lattice prune`
does.

## Edit the config

The **Config** view edits `lattice.json` as a form, with sections for
workspaces, engines, tasks, overrides and settings, or as text. Every control
names the `lattice.json` key it writes.

Both edit the file's text rather than a parsed copy, so a key the app does not
know, your key order and your formatting survive a save. Validation is the same
parse a run does, running as you type, so the editor never writes a config
`lattice run` would reject.

## Follow the system theme

The sidebar's theme control offers light, dark, and following the system, which
changes live when the OS does. The native window frame follows it.

## Related pages

- [CLI reference](/lattice/docs/cli) for every flag the run controls stand in
  for.
- [Configuration](/lattice/docs/configuration) for the fields the form writes.
- [Driver detection](/lattice/docs/drivers) for where each workspace's tasks
  come from.
