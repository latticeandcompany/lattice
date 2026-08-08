---
title: The desktop app
description: A window over the same engine — task list, dependency graph, and config editor.
group: Guides
order: 8
---

# The desktop app

Lattice has a window. It does what the CLI does, and shows a few things a terminal
cannot keep on screen: which workspaces exist and what each one resolves to, the shape
of the graph a task will run, and which inputs moved when a task missed the cache.

It is a front end, not a second Lattice. The engine is linked into it directly, so the
window and `lattice run` share one scheduler, one cache, and one set of driver rules.
There is nothing it can disagree with the CLI about.

## Running it

The app is not published as an installer yet. To run it from a clone:

```sh
cd apps/desktop
npm install
npm run app
```

That starts the Vite dev server and compiles the Rust backend, then opens the window.
The first compile takes a while; later ones are incremental.

Building the app needs the platform's webview toolchain. macOS and Windows have one
already. On Linux, install `libwebkit2gtk-4.1-dev` and the GTK development packages
your distribution names.

## Opening a repo

Pick a directory. Lattice walks up from it looking for a `lattice.json`, the same way
every command does, so choosing a subdirectory works.

A directory with no config opens the setup wizard instead of failing. The wizard reads
the repo, proposes a workspace for every directory holding a manifest it recognizes and
an engine for every tool version the repo already pins, and shows you the exact file it
is about to write. A candidate with no resolved driver is offered but not pre-selected —
Lattice would have nothing to run there yet.

What it writes is what `lattice init` writes: `lattice.json`, a committed
`.lattice/schema.json` so your editor can validate the config, and three lines appended
to `.gitignore`.

The open repo sits at the top of the sidebar, and it is also the control that changes
it: click it for every repo you have opened, plus a way to open another one or close
this one. Switching does not restart anything — the window reads the new repo and
redraws.

## Tasks

One card per workspace, in the order `lattice.json` declares them, with one row per task
that resolves to a command there. A workspace whose toolchain has no `lint` shows no
`lint` row, which is the same thing that happens on the command line — the task is
skipped rather than failed.

Each card carries the logo of the ecosystem its driver belongs to, so a repo with forty
workspaces can be scanned by shape rather than read. Every ecosystem Lattice detects has
one; a task runner with no ecosystem of its own gets a monogram instead.

Each row runs on its own, or the Run button runs everything the selection covers.
⌘-click the task tabs to stack several the way `lattice run lint test build` does.

**Cached results** is three choices rather than two switches, because the two underlying
flags overlap:

| Choice | What it does | On the command line |
| --- | --- | --- |
| Use the cache | Reuses anything unchanged, and saves what runs. | `lattice run build` |
| Run it all again | Runs everything, then saves the new results. | `--force` |
| Skip the cache | Runs everything, and saves nothing. | `--no-cache` |

The difference between the last two matters more than it looks: the middle one replaces
a stale entry, and the last leaves whatever is there untouched.

**Stop** ends a run the way Ctrl-C does — scheduling stops, children are terminated, and
the run reports as interrupted rather than failed.

When a task misses the cache, the row lists which of the key components changed:
`inputs`, `command`, `toolchain`, and so on. A key on its own can only tell you a task
missed. See [Cache internals](/lattice/docs/cache-internals) for what each component
covers.

## Graph

The dependency graph for whatever tasks are selected, laid out left to right in
dependency order. Click a task to focus it: everything it depends on and everything that
depends on it stays lit, and the rest dims. That closure is the slice of the repo a
change to that task can affect.

While a run is going the graph fills in live, so you can watch the build move through it.

Encoding is deliberately not all colour. Position carries dependency depth, a rounded
square is a persistent task, a dashed outline means the task came along as a
prerequisite rather than because you asked for it, a faded node came back from cache, and
a crimson outline is a failure. The legend under the graph names all of it.

The **List** tab shows the same tasks as a table in dependency order. A canvas cannot be
read by a screen reader or walked with a keyboard; the table can.

## Config

Two ways to edit the same file. The form covers workspaces, tasks, engines, and settings;
the JSON tab is the whole file.

The form asks in English — "files it reads", "keeps running until stopped", "how much of
the disk it may use" — and prints the `lattice.json` key each control writes beside it.
You can use the form without having read the schema, and you come away having read it.

Three-way controls where you might expect a switch are not an oversight. `persistent`
and `cache` each have a default that is not always "no", so leaving a key out of the file
is a third state, and "leave it to Lattice" is what that state is called here.

Both edit the file's text rather than a parsed copy of it, which matters more than it
sounds. Lattice rejects unknown keys in `lattice.json`, so a key a newer version
understands and this app does not is still yours — and a form that parsed the file and
wrote it back out would silently delete it, along with your key order and formatting.
Editing the text means only the bytes you changed get rewritten.

Saving validates first, using the same parser a run does, so the editor never writes a
config that `lattice run` would then reject.

## Dark mode

Light, dark, or follow the system, from the control at the bottom of the sidebar. The
choice persists, and the native window frame follows it.
