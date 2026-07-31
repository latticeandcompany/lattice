---
title: Output and logging
description: What lattice run prints in each mode, why, and how to change it.
group: Concepts
order: 8
---

# Output and logging

`lattice run` prints one of two ways: a live terminal display that settles into
a short summary, or a plain stream of `workspace:task:` lines. Lattice picks
between them once, before anything prints.

## How the mode is decided

Lattice picks **raw** output if stdout is not a terminal, or the `CI`
environment variable is set, or you passed `-l`/`--loquacious` (hidden alias
`-v`/`--verbose`), or `settings.loquacious` is `true` in `lattice.json`.
Otherwise you get **interactive** mode. `CI` counts as set whatever its value,
including empty.

```json
{
  "settings": {
    "loquacious": true
  }
}
```

The triggers combine with OR — any one of them is enough, and none can be
reversed. There is no flag that forces interactive mode back on once a pipe or
`CI` applies.

One thing overrides an interactive pick after the fact: if the tasks you're
running pull a `persistent: true` task into the graph, Lattice switches to raw
even on a real terminal. The live display repaints in place and cannot render a
process that streams indefinitely. See [Persistent
tasks](/lattice/docs/persistent-tasks) for the rest of what `persistent`
changes.

## Interactive mode

On a terminal, with no `CI` and no `-l`, a run looks like this (captured from
this repo's own `lattice run check`, with the cache cleared first — the teal `❖`
and green `✓` are real terminal color, flattened here):

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

A cache hit settles the same way, with a teal `●` instead of `✓` and the short
cache key in place of the duration:

```text
● lattice-cache:check          cache hit [155cfd2a]
```

When every task in the run came back from cache, one more line follows the
summary — `❖❖❖ FULL CACHE`, painted across the teal ramp (`teal-700` →
`teal-500` → `teal-300`) a character at a time:

```text
● lattice-cache:check          cache hit [155cfd2a]
● lattice:check                cache hit [536a1348]
────────────────────────────────────────────────────
❖  7 tasks · 7 cached · 0 failed  0.07s

❖❖❖ FULL CACHE
```

It prints only when nothing executed: at least one task was scheduled, none
failed, and every one was a hit. A run with a single miss, a failure, a
`persistent: true` task in the graph, or a filter that matched no workspace does
not get it.

A skipped task (a dependency of a failed task, under `--continue`) settles with
a dim `○`; a failed one with a red `✗` and `FAILED` in place of the duration.
Whatever a task printed while it ran is not shown live. It's collected and, if
the task fails, dumped underneath a `✗ workspace:task output` header once the
run reaches it. A task that succeeds never shows its output in this mode.

The header line, the rule under it, the spinners, and the closing summary all
repaint in place. Once the run ends, only the settled lines and the summary are
left behind.

## Raw / CI mode

Off a terminal, under `CI`, or with `-l`, the same run prints one line per
event, in the order events arrive, with no cursor movement:

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

A cache hit prints `workspace:task: cache hit [<key>]` in place of `done`, and a
failed one prints `workspace:task: FAILED`. A skipped task prints
`workspace:task: skipped (<reason>)` only under `-l`; without it, a skipped task
prints nothing here. Every line is the label plus plain text, safe to grep or
feed to another tool.

Without `-l`, a task's own output is collapsed here too: you get the
`running`/`done` lines but not the command's stdout or stderr, unless the task
fails, in which case its captured lines print underneath, each still prefixed
`workspace:task:`. `-l` turns those lines on for every task, and adds trace
lines Lattice doesn't otherwise show — `lattice: running <task> across N
workspace(s)` at the start, and a `lattice: workspace:task: hash <key>` line
per task right before it looks up the cache:

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
lattice: full cache — nothing to run
```

That last line is the raw-mode form of interactive mode's `FULL CACHE` banner,
under the same rule and with no color, so a piped run or a CI log can be
grepped for it.

`-l` and raw-because-not-a-terminal are the same reporter; the only difference
`-l` makes to a piped or CI run is turning on those extra lines. On a terminal,
`-l` is also one of the mode triggers, so it's how you get this line-by-line
stream at an interactive shell.

### Label colors

On a terminal, the `workspace:task` label at the head of each line is colored,
and every task in the run gets its own color. Tasks run in parallel, so their
lines interleave; the color is what lets you follow one task down a screen of
eight.

The palette is eight hues one 45° step apart around the wheel, all at the same
saturation and lightness. It starts at 25° so nothing in it reads as the red a
`FAILED` marker uses.

Colors are handed out in the order labels are first seen, so the first eight
distinct `workspace:task` pairs in a run never share a color; a ninth wraps back
to the first. Because assignment follows first-seen order and tasks start in
parallel, which color a given task gets can differ between runs. Within one run
it never changes.

Both halves of the label count, so `web:build`, `web:test`, and `api:build` are
three different colors. Trace lines get the same treatment: the `web:build`
inside `lattice: web:build: hash …` carries that task's color.

Nothing else on the line is styled. The message text after the label stays your
terminal's default and `FAILED` is still the word `FAILED`, so nothing here is
conveyed by color alone. Off a terminal the labels print bare.

## Persistent output always streams

A `persistent: true` task forces the raw stream, and inside it that task's own
output lines print the moment they're produced, with or without `-l`. The
collapse-on-success behavior above applies to ordinary, one-shot tasks. See
[Persistent tasks](/lattice/docs/persistent-tasks).

## Color

Color depends on whether stdout is a real terminal, not on which mode you got.
Both modes color when it is; neither does when it isn't. An `-l` run at your
shell has colored labels; the same run piped into `cat`, into a file, or
through a CI log emits no escapes at all.

`NO_COLOR` (any value; see <https://no-color.org/>) turns color off everywhere,
in either mode, with no other change to the layout. The color decision is made
once per run, after the mode is final and before anything prints, which is why a
`persistent: true` task forcing raw mode still keeps its color.

What each mode spends color on differs. Interactive mode uses the teal accent on
the header, the rosette, spinners, and cache hits, plus green `✓`/red `✗` on
results. Raw mode colors exactly one thing: the `workspace:task` label at the
head of every line.

## stdout versus stderr

In raw mode the streams split like this:

| Line | Stream |
| --- | --- |
| `running`, `cache hit`, `done`, final summary | stdout |
| `skipped` and `note` trace lines (both `-l` only) | stdout |
| `FAILED`, `warn` lines | stderr |
| A task's own output | whichever stream the task wrote it to, including on a later replay after failure |

So `lattice run build 2>/dev/null` still shows every task's progress and its
stdout output, and drops only warnings, `FAILED` markers, and stderr output. In
interactive mode the live display goes to stdout while failure output goes to
stderr, so redirecting stdout away still leaves the failure dump on the terminal.

## Trace lines (`note`/`warn`)

Two categories of line exist outside the per-task events above. `note` carries
hashing, cache, and toolchain trace detail; it is dropped silently in raw mode
without `-l`, and shown dim in interactive mode regardless (the `hash <key>`
lines above are `note` lines). `warn` always prints in both modes, prefixed
`lattice: warning:` in raw mode and with a yellow `warn` label in interactive
mode.

## Related pages

- [Continuous integration](/lattice/docs/continuous-integration) — running
  Lattice on a build machine.
- [Persistent tasks](/lattice/docs/persistent-tasks) — what pulls a run into
  raw mode, and what else `persistent` changes.
