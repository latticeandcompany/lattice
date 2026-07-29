---
title: Output and logging
description: What lattice run prints in each mode, why, and how to change it.
group: Concepts
order: 8
---

# Output and logging

`lattice run` prints one of two ways: a live terminal display that settles
into a short summary, or a plain stream of `workspace:task:` lines. Which one
you get is decided once, at the start of the run, from your terminal and your
environment — not something you configure per project.

## How the mode is decided

Lattice picks **raw** output if any of these is true:

- stdout is not a terminal (piped, redirected, or run from something that
  isn't attached to one)
- the `CI` environment variable is set
- you passed `-l`/`--loquacious` (or its hidden alias `-v`/`--verbose`), or
  `settings.loquacious` is `true` in `lattice.json`

Otherwise you get **interactive** mode — the live display. This is
`detect_mode` in `crates/lattice-output/src/lib.rs:29`, driven by
`detect_output_mode` in `crates/lattice/src/cli.rs:131`, which checks
`console::user_attended() && console::Term::stdout().is_term()` for the TTY
test and `std::env::var("CI")` for the CI test.

```json
{
  "settings": {
    "loquacious": true
  }
}
```

Flag and setting combine with OR: either one is enough to force raw output,
and neither can force interactive mode back on once a real trigger (no TTY,
or `CI`) applies. There's no flag to force interactive mode when Lattice
would otherwise pick raw — a pipe or `CI` always wins.

One case overrides an interactive pick after the fact: if the tasks you're
running pull a `persistent: true` task into the graph, Lattice switches to
raw even on a real terminal, because a dev server's streaming output can't be
rendered inside the live display. See
[Persistent tasks](/lattice/docs/persistent-tasks) for the full rule.

## Interactive mode

On a terminal, with no `CI` and no `-l`, a run looks like this (captured from
this repo's own `lattice run check`, with the cache cleared first — the
teal `❖` and green `✓` are real terminal color, flattened here):

```text
❖ lattice  check  · 8 workspaces
────────────────────────────────────────────────────
⠙ lattice-cache:check          running…
⠙ lattice-runner:check         running…
⠙ dagger:check                 running…
⠙ lattice-output:check         running…
⠙ lattice-workspace:check      running…
⠙ lattice-config:check         running…
⠙ lattice:check                running…
```

Each running task gets its own spinner line, labeled `workspace:task`. As a
task finishes it settles into a static line in place of its spinner:

```text
✓ dagger:check                 0.20s
✓ lattice-output:check         0.18s
✓ lattice-workspace:check      0.20s
✓ lattice-config:check         0.22s
✓ lattice-cache:check          0.21s
✓ lattice-runner:check         0.25s
✓ lattice:check                0.24s
────────────────────────────────────────────────────
❖  7 tasks · 0 cached · 0 failed  0.34s
```

A cache hit settles the same way, with a teal `●` instead of `✓` and the
short cache key in place of the duration:

```text
● lattice-cache:check          cache hit [155cfd2a]
```

A skipped task (a dependency of a failed task, under `--continue`) settles
with a dim `○`; a failed one with a red `✗` and `FAILED` in place of the
duration. Whatever a task printed while it ran is not shown live — it's
collected and, if the task fails, dumped underneath a `✗ workspace:task
output` header once the run reaches it. A task that succeeds never shows its
output at all in this mode. This is the collapse-and-surface-on-failure rule:
you see progress and a result for everything, and the actual command output
only for what went wrong.

The header line, the rule under it, the spinners, and the closing summary are
all built with `indicatif::MultiProgress` in `InteractiveReporter`
(`crates/lattice-output/src/lib.rs:419`); nothing in this mode is meant to be
piped or grepped; it repaints in place and leaves only the settled lines and
the summary behind once the run ends.

## Raw / CI mode

Off a terminal, under `CI`, or with `-l`, the same run prints one line per
event, in the order events arrive, with no cursor movement and no color:

```text
$ lattice run check | cat
dagger:check: running
lattice-cache:check: running
lattice-output:check: running
lattice-config:check: running
lattice-runner:check: running
lattice-workspace:check: running
lattice:check: running
lattice-output:check: done (0.23s)
lattice-workspace:check: done (2.60s)
lattice-cache:check: done (3.86s)
dagger:check: done (4.02s)
lattice-config:check: done (4.03s)
lattice-runner:check: done (5.93s)
lattice:check: done (8.33s)
lattice: 7 tasks, 0 cached, 0 failed, 8.41s
```

A cache hit prints `workspace:task: cache hit [<key>]` in place of `done`; a
skipped task prints `workspace:task: skipped (<reason>)`; a failed one prints
`workspace:task: FAILED`. This is `CiReporter` in
`crates/lattice-output/src/lib.rs:311` — every line is
`println!`/`eprintln!` with no styling, safe to grep or feed to another tool.

Without `-l`, a task's own output (what its command printed) is collapsed
here too — you get the `running`/`done` lines but not the command's stdout or
stderr, unless the task fails, in which case its captured lines print
underneath, each still prefixed `workspace:task:`. `-l` turns on those lines
for every task, not just failed ones, and adds a few trace lines Lattice
doesn't otherwise show — `lattice: running <task> across N workspace(s)` at
the start, and a `lattice: workspace:task: hash <key>` line per task right
before it looks up the cache:

```text
$ lattice run check -l
lattice: running `check` across 8 workspace(s)
lattice: lattice-config:check: hash 5d7971720252736c
lattice: lattice-cache:check: hash 155cfd2a137198a7
lattice: lattice-runner:check: hash a6745dea3308d74b
lattice: dagger:check: hash 7639609d599c4218
lattice: lattice-workspace:check: hash 83711b2f0b8a853b
lattice: lattice-output:check: hash 3ac2770f88dfc055
lattice-runner:check: cache hit [a6745dea]
lattice: lattice:check: hash 098ed747b89a4057
lattice:check: cache hit [098ed747]
lattice-cache:check: cache hit [155cfd2a]
lattice-config:check: cache hit [5d797172]
dagger:check: cache hit [7639609d]
lattice-workspace:check: cache hit [83711b2f]
lattice-output:check: cache hit [3ac2770f]
lattice: 7 tasks, 7 cached, 0 failed, 0.08s
```

`-l` and raw-because-not-a-terminal are the same reporter; the only
difference `-l` makes to a piped or CI run is turning on these extra lines.
`-l` on an actual terminal has the same effect: it forces raw mode there too
(that's one of the three triggers above), so `-l` is also how you get this
line-by-line stream while sitting at an interactive shell — for copying
output, or watching a dev server's log inline instead of the live display.

## Persistent output always streams

A `persistent: true` task's own output streams live the moment it's
produced, in both modes — dev server logs and watcher output are the reason
the task exists, so Lattice never buffers or collapses them, even in
interactive mode's otherwise-quiet middle. Everything else in this page's
collapse-on-success rule is about ordinary, one-shot tasks. See
[Persistent tasks](/lattice/docs/persistent-tasks) for what a persistent task
changes about a run beyond its output.

## Color

Color renders only in interactive mode. `CiReporter` (raw mode) never calls
into styling at all — its lines are built with plain `format!`, so a piped
run, a `CI` run, or an `-l` run never emits ANSI escapes regardless of
terminal capability, and a log file or a CI viewer never has to strip them.
`InteractiveReporter` styles its output via the `console` crate, which honors
`NO_COLOR` (any value; see <https://no-color.org/>) automatically: exporting
it turns color off in an otherwise-interactive session with no other change
to the mode or the layout. Confirmed by running the same task under a real
terminal with and without `NO_COLOR=1` — the escape codes disappear, nothing
else does. `should_enable_color` in `crates/lattice-output/src/lib.rs:41`
states this as the rule Lattice's output follows: color only for a genuine
interactive session with `NO_COLOR` unset.

## stdout versus stderr

This is what matters if you're piping a run into something else. In raw
mode, `CiReporter` (`crates/lattice-output/src/lib.rs:311`) sends:

- `running`, `cache hit`, `done`, `skipped`, and the final summary line to
  **stdout**
- `FAILED`, `warn` lines, and a task's own stderr output to **stderr**
- a task's own stdout output to stdout, its stderr output to stderr — each
  line keeps the stream it was originally written to, including when it's
  replayed later because the task failed

So `lattice run build 2>/dev/null` still shows you every task's progress and
its stdout output, and only drops warnings, `FAILED` markers, and stderr
output. In interactive mode, the equivalent split exists but matters less in
practice — the live display itself goes to stdout via
`indicatif::MultiProgress`, and failure output is written with `eprintln!`
through `mp.suspend`, so redirecting stdout away still leaves you the
failure dump on the terminal.

## Trace lines (`note`/`warn`)

Two categories of line exist outside the per-task events above. `note`
carries hashing/cache/toolchain trace detail and is shown only when
loquacious — silently dropped in raw mode without `-l`, and shown dim in
interactive mode regardless (the `hash <key>` lines above are `note` calls).
`warn` always prints, in both modes, prefixed `lattice: warning:` in raw
mode and with a yellow `warn` label in interactive mode.

## Related pages

- [Continuous integration](/lattice/docs/continuous-integration) covers
  running Lattice in CI specifically — where `CI` gets set, and what a
  greppable log buys you there.
- [Persistent tasks](/lattice/docs/persistent-tasks) covers the forcing rule
  in full: what pulls a task into "persistent", and what else changes about
  a run beyond its output.
