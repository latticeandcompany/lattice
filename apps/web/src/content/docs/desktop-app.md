---
title: The desktop app
description: Build the window, open a project, and run tasks from the list, the graph, or the config editor.
group: Guides
order: 8
---

# The desktop app

Lattice has a window. It runs the tasks `lattice run` runs, and it shows three
things a terminal cannot hold on screen: every workspace and what each one
resolved to, the shape of the graph a task will run, and which part of the cache
key moved when a task missed.

The engine is linked into the app, so the window and the CLI share one
scheduler, one cache, and one set of driver rules. There is nothing the two can
disagree about.

## Build and run it from a clone

No installer is published yet. Build it yourself:

```sh
cd apps/desktop
npm install
npm run app
```

That starts the frontend dev server, compiles the Rust backend, and opens the
window. The first compile takes a while and later ones are incremental.

To produce an installer for your own platform, run `npm run bundle` instead. It
writes an `.app` and a `.dmg` on macOS, an `.msi` on Windows, and a `.deb` and
an AppImage on Linux.

Either command needs the platform's webview toolchain. macOS and Windows have
one already. On Debian or Ubuntu, install what CI installs:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

## Open a project

A project is one repo with a `lattice.json` at its root. Click **Open a
project…** in the sidebar and pick a directory in the OS dialog, titled **Open a
Lattice project**. Lattice walks up from what you picked looking for a
`lattice.json`, the same way every command does, so picking a subdirectory
works.

Pick a directory with no config anywhere above it and the app opens **Set up
this project** instead of failing. That is a four-step walkthrough:

1. **Project root** confirms where `lattice.json` goes, and offers **Choose
   another directory**.
2. **Workspaces** lists every directory in the repo that holds a manifest, one
   tick box each, and names the marker file it found and the driver it resolved.
   A candidate with no driver is offered but left unticked, because declaring it
   would halt the next run. The repo root, when it is one of the candidates,
   starts unticked too.
3. **Engines** lists every tool version the repo already pins at its root, read
   out of `.tool-versions`, `.nvmrc`, `package.json`, `go.mod`, and the
   `.python-version` family, and says which file each came from. Tick the ones
   to copy into `engines`, verbatim.
4. **Root config** shows the exact `lattice.json` about to be written, and
   redraws it as you change a tick above. **Write lattice.json** commits it and
   opens the project.

The preview comes from the same code path `lattice init` uses, so the file the
window writes is the file the CLI would have written: `lattice.json`, a
committed `.lattice/schema.json` so your editor can check the config as you
type, and three lines appended to `.gitignore`.

The project block at the top of the sidebar is also the control that changes it.
Click it for every project you have opened, plus **Open another project…** and
**Close this project**. Switching restarts nothing; the window reads the new
project and redraws. **Reload from disk** underneath re-reads the config after
you edit it outside the app.

## Run tasks

The **Tasks** view is one card per workspace, in the order `lattice.json`
declares them, with one row per task that resolves to a command there. A
workspace whose driver has no `lint` shows no `lint` row, which is what happens
on the command line too: the task is skipped, not failed.

Each card carries the mark of its driver's ecosystem, so a repo with forty
workspaces can be scanned by shape rather than read. Thirteen ecosystems have
artwork; a task runner that belongs to none gets a two-letter monogram. The chip
beside it names the driver, and its tooltip says whether the driver came from a
declaration in `lattice.json` or from a file in the directory.

A task row carries a `persistent` chip when the task declares `persistent: true`
and a `never cached` chip when it declares `cache: false`. The row has two
buttons of its own, both scoped to that one workspace with `--filter`: one runs
the task, the other runs it again ignoring what is cached. The **Run** button in
the bar above runs whatever the task tabs have selected. Cmd-click or Ctrl-click
the tabs to stack several, the way `lattice run lint test build` does.

The rest of the bar maps onto `lattice run` flags:

| Control | Flag |
| --- | --- |
| **Cache mode**: **Use cache** | none |
| **Cache mode**: **Force** | `--force` |
| **Cache mode**: **No cache** | `--no-cache` |
| **Filter by workspace name** | `-f`/`--filter` |
| **Concurrency**, default **One per CPU** | `--concurrency` |
| **Keep going after a failure** | `--continue` |
| **Finish each task before starting the next** | `-s`/`--sequentially` |

Cache mode is three exclusive choices rather than two switches because the flags
overlap. **Force** runs every task and replaces its stored result. **No cache**
runs every task and stores nothing, so whatever is already cached stays as it
was. Two independent switches would let you ask for "write but do not read",
which the engine cannot express.

**Finish each task before starting the next** is about task graphs, not about
parallelism: it runs `lint` everywhere to completion, then `build` everywhere.
To run one task at a time, set **Concurrency** to **1 at once** instead. See
[Selecting what runs](/lattice/docs/filtering).

**Stop** ends a run the way Ctrl-C does. Scheduling stops, running children are
terminated, and the run reports as interrupted rather than failed.

When a task misses the cache, a row of chips under it reads `cache miss:`
followed by the parts of the key that moved: `inputs`, `command`, `toolchain`,
and so on. A key on its own can only tell you that a task missed. See [Cache
internals](/lattice/docs/cache-internals) for what each name covers.

Output is collected per task and opens on its own when a task fails. Any other
task's output is behind the chevron on its row.

## Read the graph

The **Graph** view draws the dependency graph for whatever tasks the tabs have
selected, left to right in dependency order. Click a task to focus it:
everything it depends on and everything that depends on it stays lit and the
rest dims, which is the slice of the repo a change to that task can reach.
**Clear focus** puts it back. **Find a task** narrows the graph by name, and the
count on the right reads `N tasks · N layers deep`. While a run is going the
graph fills in live.

The encoding is deliberately not all color, and the legend under the canvas
names all of it: left to right is dependency order, filled means the task ran,
an outline means it did not, faded is a cache hit, a rounded square is a
persistent task, a dashed outline means the task came along as a prerequisite
rather than because you asked for it, and an amber outline is a failure.

**List** shows the same tasks as a table in dependency order, with each one's
resolved command. A canvas cannot be read by a screen reader or walked with a
keyboard; the table can.

## Edit the config

The **Config** view has two tabs over one file. **Form** covers the project
settings, engines, workspaces, and tasks. **JSON** is the whole file in a text
area.

Every control in the form carries a domain noun and the `lattice.json` key it
writes, so you can use it without having read the schema and come away having
read it:

| Label | Key |
| --- | --- |
| **Lattice version** | `latticeVersion` |
| **Cache directory** | `settings.cacheDir` |
| **Cache size limit** | `settings.maxCacheSize` |
| **Driver detection** | `auto`, on a workspace |
| **Dependencies** | `dependsOn`, on a task |
| **Inputs** | `inputs` |
| **Outputs** | `outputs` |
| **Persistent** | `persistent` |
| **Cache** | `cache` |

**Persistent** and **Cache** offer **Not set**, **Yes**, and **No** rather than
a switch. Leaving a key out of the file is a third state, and neither key
defaults to no, so a switch sitting in its off position would be a lie about
what the file says.

Both tabs edit the file's text, never a parsed copy of it. Lattice rejects
unknown keys in `lattice.json`, so a key a newer version understands and this
build does not is still yours: a form that parsed the file and wrote it back out
would delete it, along with your key order and your formatting. Editing the text
means only the bytes you changed are rewritten.

The header says **Unsaved changes** or **Saved**, and **Save** stays unavailable
while there is a problem to fix. Validation is the same parse a run does, run in
the backend as you type, so the editor never writes a config `lattice run` would
then reject. A file that is not valid JSON at all disables the form and says so;
fix it in the **JSON** tab.

## Follow the system theme

The control at the bottom of the sidebar offers **Light**, **Dark**, and
**System**. **System** tracks the OS and changes live when it does. The choice
persists across launches, and the native window frame follows it.

## Related pages

- [CLI reference](/lattice/docs/cli) for every flag the run bar stands in for.
- [Configuration](/lattice/docs/configuration) for the fields the form writes.
- [Driver detection](/lattice/docs/drivers) for how a workspace gets the driver
  its card names.
