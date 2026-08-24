---
title: Output and logging
description: What lattice run prints in each of its two modes, and how to get the other one.
group: Concepts
order: 8
---

# Output and logging

`lattice run` prints one of two ways. **Interactive** is a live display that
repaints in place while the run goes. **Raw** is one plain line per event, in
the order they arrive, with no cursor movement anywhere. Lattice picks one
before anything prints and keeps it for the whole run.

Every transcript on this page is a run of the same repo: four workspaces named
`ui`, `web`, `api`, and `docs`, where `web` and `api` depend on `ui`, and a
`build` task that declares `inputs` and `outputs`.

## Check which mode you will get

Interactive is what a terminal gets. Any one of these gives you raw instead, and
none of them can be reversed once it applies:

- stdout is not a terminal, so a pipe, a redirect, or a CI step.
- `CI` is set to any value, including the empty string.
- You passed `-v`, or its long form `--verbose`.
- `lattice.json` sets `settings.loquacious`.
- The run pulls a `persistent: true` task into its graph. A dev server streams
  for as long as it is up, and the live display repaints in place, so it cannot
  show one. See [Run dev servers](/lattice/docs/dev-servers).

To make every run in a repo print raw lines, put it in the config rather than in
everyone's shell history:

```json
{
  "settings": {
    "loquacious": true
  }
}
```

There is no flag that forces the live display back on. `-v` on the command line
beats a `false` setting, because the two are combined with OR.

## What each mode prints for the same event

The runner reports events; the two modes are two renderings of the same list.
Read across a row to translate a line you have in front of you.

| Event | Raw | Interactive |
| --- | --- | --- |
| The run starts | ``lattice: running `build` across 4 workspaces`` (`-v` only) | `❖ lattice  build  · 4 workspaces` |
| A task starts | `ui:build: running` | `⠋ ui:build  running…` |
| It finishes | `ui:build: done (1.02s)` | `✓ ui:build  1.02s` |
| It comes back from cache | `ui:build: cache hit [5341be25]` | `● ui:build  cache hit [5341be25]` |
| It fails | `ui:build: FAILED (code 3) after 1.02s` | `✗ ui:build  FAILED (code 3) 1.02s` |
| A prerequisite failed, so it never starts | `web:build: skipped (dependency failed)` (`-v` only) | `○ web:build  skipped (dependency failed)` |
| A persistent task exits cleanly | `docs:dev: exited (code 0) after 2.02s` | raw only |
| A persistent task exits non-zero | `docs:dev: EXITED (code 1) after 2.02s` | raw only |
| A trace line | `lattice: ui:build: hash 5341be25…` (`-v` only) | not shown |
| A warning | `lattice: warning: ui:build: …` | `warn ui:build: …` |
| The run ends | `lattice: 4 tasks, 0 cached, 0 failed, 3.08s` | `❖  4 tasks · 0 cached · 0 failed  3.08s` |
| The run ends and something hit | `lattice: 4 tasks, 1 cached, 0 failed, 3.09s, 3.02s saved` | `❖  4 tasks · 1 cached · 0 failed  3.09s · 3.02s saved` |
| Every task came back from cache | `lattice: full power, nothing to run` | `❖❖❖ FULL POWER` |

The two persistent rows have no interactive form. A run that can reach a
persistent task is switched to raw before it starts, so the live display never
gets to render one exiting. The trace row has no interactive form either. The
hash and cache-miss lines belong to `-v`, and the live display leaves them out.

How much a failure line carries depends on how the task died. A command that ran
and returned an exit code gets both halves, the code and the time. A task a
signal killed, and a task Lattice stopped for overrunning its `timeout`, have no
exit code, so the line carries the time alone: `ui:build: FAILED after 30.00s`.
A task that failed before its command ever started has neither, and the line is
the bare word `FAILED`. Its cache key would not compute, or its shell would not
spawn. It never ran, so there is no time to report.

Four details the table cannot hold. The run header lists every distinct task
name in the graph, sorted and joined with `+`, so `lattice run lint build` heads
its run `build+lint` in both modes, and so does a `lattice run lint` whose
`lint` depends on `build`. The workspace count is a real plural, so a run in one
workspace says `across 1 workspace`. Interactive pads the `workspace:task` label
to 28 characters and truncates a longer one with `…`, which is what lines the
glyphs and durations up in a column. And a duration under a minute reads as
seconds (`1.02s`); past that it switches to clock form, `4:07` and then
`1:12:30`.

## What the saved figure counts

A run with cache hits ends its summary with a saved figure. It is a sum of one
number per hit: how long the run that wrote that cache entry spent producing it,
recorded in the entry's metadata at the time. When the total comes to zero the
tail is left off, so a run with no hits reads the way it always has.

That makes it task time, not wall clock. Four cached one-minute tasks with
nothing between them in the graph add up to `4m 00s saved`, even though running
them would have cost you about a minute of waiting. It is a count of work not
repeated. The elapsed time next to it is the clock.

Past a minute the two figures are written differently. Under a minute they match
(`4.27s`). Above it, saved time reads `4m 07s`, then `14h 22m` once it crosses
an hour, dropping the seconds — where the elapsed time switches to clock form,
`4:07` and then `1:12:30`.

Each run's figure is also appended to a ledger kept with the cache.
[`lattice stats`](/lattice/docs/cli) adds them up across every run the repo has
recorded.

## Read the live display

A run at a terminal, mid-flight. Every task that has started has a line, in the
order they started, and a task that has already ended keeps its line where it
was until the run is over:

```text
❖ lattice  build  · 4 workspaces
────────────────────────────────────────────────────
⠸ docs:build                   running…
✓ ui:build                     1.02s
⠙ api:build                    running…
⠋ web:build                    running…
```

`ui:build` sits second because it started second, not because of where it
finished. `web` and `api`, which depend on it, joined underneath once it was
done. A task's line settles in place when it ends: `✓` and its duration on
success, `✗ FAILED` with the exit code and the elapsed time on failure, a teal
`●` and the short cache key on a hit.

Nothing above those lines reports a hash or a cache miss. The task's own line
already carries both. A hit prints its abbreviated key, and a miss shows up as
the task running, so the live display leaves the trace to `-v` and the raw
stream.

Whatever a task prints while it runs is collected rather than shown. A task that
succeeds never shows its output in this mode. A task that fails gets its output
printed under a header once the run reaches it:

```text
❖ lattice  build  · 4 workspaces
────────────────────────────────────────────────────
● docs:build                   cache hit [94477519]
○ web:build                    skipped (dependency failed)
○ api:build                    skipped (dependency failed)

✗ ui:build output
    ui: building 3 files
    ui: cannot resolve module 'styles'
────────────────────────────────────────────────────
❖  2 tasks · 1 cached · 1 failed  1.02s · 3.02s saved
```

The captured lines read in the order the task produced them, whichever stream
each one came from, and they print at full brightness rather than dimmed. A
compiler's error and the file it was compiling stay next to each other, because
Lattice appends every line from both pipes to one buffer as it reads them. That
is arrival order as Lattice reads it. A task that dumps a whole stdout buffer and
a whole stderr buffer in one burst can still print one stream before the other.
Output that arrives spread over time is in the order it happened.

That is the whole screen after the run, not a frame from the middle of it.
Lattice clears the live region when the run ends, so the header, the cache hits,
the skipped tasks, the failure output, and the summary stay, and the `✓` and `✗`
lines go with the region they were drawn in. The counts in the summary are the
record of what ran.

Two tasks in that summary, not four: `web:build` and `api:build` were skipped
because `ui:build` failed, and a skipped task is counted separately from one
that ran. See [Selecting what runs](/lattice/docs/filtering) for `--continue`,
which is what let the run get that far.

When every task came back from cache, a banner follows the summary:

```text
❖ lattice  build  · 4 workspaces
────────────────────────────────────────────────────
● docs:build                   cache hit [94477519]
● ui:build                     cache hit [5341be25]
● api:build                    cache hit [1ccd0b3e]
● web:build                    cache hit [d25c0cf0]
────────────────────────────────────────────────────
❖  4 tasks · 4 cached · 0 failed  0.00s · 8.12s saved

❖❖❖ FULL POWER
```

The banner needs at least one task scheduled, no failures, and a hit for every
one. A single miss, a failure, a persistent task in the graph, or a `--filter`
that matched no workspace all leave it out.

It says `FULL POWER` and not `FULL CACHE`. A full cache is how a disk running out
of room is described, which is the opposite of what just happened.

`8.12s` against an elapsed `0.00s` is the task-time reading at work: those four
builds took 8.12 seconds of work between them when they last ran, spread over
about three seconds of waiting.

## Read the raw stream

Pipe the same run and you get one line per event, prefixed `workspace:task:`:

```text
$ lattice run build | cat
ui:build: running
docs:build: running
ui:build: done (1.02s)
api:build: running
web:build: running
docs:build: done (3.02s)
web:build: done (2.04s)
api:build: done (2.04s)
lattice: 4 tasks, 0 cached, 0 failed, 3.08s
```

Lines arrive as events happen, so they interleave: `docs:build` starts second
and finishes last. Nothing repaints, nothing is erased, and every line is a
label plus plain text, which is what makes the stream safe to grep or feed to
another tool.

A run with nothing to do is one line per task, then the summary:

```text
$ lattice run build | cat
docs:build: cache hit [94477519]
ui:build: cache hit [5341be25]
api:build: cache hit [1ccd0b3e]
web:build: cache hit [d25c0cf0]
lattice: 4 tasks, 4 cached, 0 failed, 0.01s, 8.12s saved
lattice: full power, nothing to run
```

`lattice: full power, nothing to run` is the raw form of the `FULL POWER`
banner, under the same rule and with no color, so a CI log can be grepped for
it.

The saved figure is appended after the elapsed time rather than inserted before
it, so a log grepped for the counts reads the way it always has.

Without `-v`, a task's own output is collapsed here too. A failure is the
exception: the captured lines print underneath the `FAILED` line, in the order
the task produced them, each still carrying the task's label.

```text
$ lattice run build --continue | cat
ui:build: running
docs:build: cache hit [94477519]
ui:build: FAILED (code 3) after 1.02s
ui:build: ui: building 3 files
ui:build: ui: cannot resolve module 'styles'
lattice: 2 tasks, 1 cached, 1 failed, 1.03s, 3.02s saved
```

`code 3` is the exit code the command returned, and `after 1.02s` is how long it
ran. A signal or a `timeout` leaves the line `ui:build: FAILED after 1.02s`, and
a task that never got as far as running its command leaves it `ui:build: FAILED`.

Nothing named `web:build` or `api:build` appears. In raw mode a skipped task is
silent unless you pass `-v`.

## Turn on the hash and cache-miss lines

`-v` adds three things to the raw stream: the run header, the per-task trace
lines, and every task's own stdout and stderr as it is produced. This is the only
place the hash and cache-miss lines appear; the live display does not carry
them.

```text
$ lattice run build -v
lattice: running `build` across 4 workspaces
lattice: docs:build: hash 944775197435b927
lattice: ui:build: hash 26be571e2ec773a7
lattice: ui:build: cache miss: inputs changed
ui:build: running
docs:build: cache hit [94477519]
ui:build: ui: built 1 file
ui:build: done (1.03s)
lattice: web:build: hash 7c91a67acc30bce0
lattice: api:build: hash c70d30d17414bd4a
lattice: web:build: cache miss: dependencies changed
web:build: running
lattice: api:build: cache miss: dependencies changed
api:build: running
web:build: web: built 1 page
api:build: api: built 1 binary
api:build: done (2.03s)
web:build: done (2.03s)
lattice: 4 tasks, 1 cached, 0 failed, 3.09s, 3.02s saved
```

The `lattice: ` prefix marks a line Lattice wrote about the run rather than a
line a task wrote. Each `hash` line prints just before the cache lookup that
uses it, and the `cache miss:` line after it names which part of the key moved.
Here an edit to `ui`'s source moved `inputs`, and `web` and `api` then missed on
`dependencies`, because a prerequisite's key is part of theirs. For what each
component name covers, see [Cache
internals](/lattice/docs/cache-internals).

A failing run under `-v` prints the failed task's output twice: once live, as
the process produces it, and once more in the replay that follows the `FAILED`
marker.

```text
$ lattice run build --continue -v
lattice: running `build` across 4 workspaces
lattice: ui:build: hash b64234148e6c7a2d
lattice: docs:build: hash 944775197435b927
lattice: ui:build: cache miss: inputs changed
ui:build: running
docs:build: cache hit [94477519]
ui:build: ui: building 3 files
ui:build: ui: cannot resolve module 'styles'
ui:build: FAILED (code 3) after 1.03s
ui:build: ui: building 3 files
ui:build: ui: cannot resolve module 'styles'
web:build: skipped (dependency failed)
api:build: skipped (dependency failed)
lattice: 2 tasks, 1 cached, 1 failed, 1.03s, 3.02s saved
```

At a terminal, `-v` is also one of the mode triggers, so it is how you get this
stream at an interactive shell rather than the live display. In a pipe or a CI
job you are already in raw mode, and `-v` only adds the extra lines. See [Run
Lattice in CI](/lattice/docs/continuous-integration).

## Follow one task down a screen of eight

At a terminal, raw mode colors the `workspace:task` label at the head of each
line, and every task in the run gets its own. Lines from tasks running in
parallel interleave, and the color is what lets you read one of them.

The palette is eight hues one 45° step apart, all at the same saturation and
lightness so no label reads as louder than another. It starts at 25° so nothing
in it can be mistaken for the red a `FAILED` marker uses. Colors go out in the
order labels are first seen, so the first eight distinct `workspace:task` pairs
never share one and a ninth wraps back to the first. Because tasks start in
parallel, which color a task gets can differ between runs; within one run it
never changes.

Both halves of the label count, so `web:build`, `web:test`, and `api:build` are
three colors. A trace line gets the same treatment: the `web:build` inside
`lattice: web:build: hash …` carries that task's color.

Nothing else on the line is styled. `FAILED` is still the word `FAILED`, so no
status here is carried by color alone.

## Turn color off

Color follows the terminal, not the mode. Both modes color when stdout is a real
terminal and neither does when it is not, so a `-v` run at your shell has
colored labels and the same run redirected to a file emits no escapes at all.

To suppress color at a terminal, set `NO_COLOR` to any value:

```sh
NO_COLOR=1 lattice run build -v
```

The layout does not change, only the escapes. Lattice decides about color once
per run, after the mode is settled and before anything prints, which is why a
`persistent: true` task forcing raw mode keeps its colored labels.

Interactive spends color on the teal accent for the header, the rosette, the
spinners, cache hits, and the saved figure in the summary, plus a green `✓` and
a red `✗` on results. The elapsed time beside the saved figure stays dim. Raw
colors exactly one thing: the label.

## Keep progress and failures apart

In raw mode the two streams split like this:

| Line | Stream |
| --- | --- |
| `running`, `cache hit`, `done`, `exited (code 0)`, the summary | stdout |
| `skipped` and trace lines, both `-v` only | stdout |
| `FAILED`, `EXITED (…)`, `lattice: warning: …` | stderr |
| A task's own output | whichever stream the task wrote it to |

So `lattice run build 2>/dev/null` keeps every task's progress and its stdout
and drops the warnings, the `FAILED` markers, and anything a task wrote to
stderr. In interactive mode the live display goes to stdout and failure output
goes to stderr, so redirecting stdout away still leaves the failure dump on the
terminal.

Two kinds of line sit outside the per-task events. A trace line carries hashing,
cache, and toolchain detail about one task. Raw mode prints it only under `-v`,
and the live display does not print it at all. A note about the run as a whole is
the exception in interactive. Provisioning a toolchain and pruning the cache
print dim, because nothing else on screen would account for the wait. A warning
always prints in both modes, prefixed `lattice: warning:` in raw and labeled with
a yellow `warn` in interactive.

## An invalid byte costs one character

A task's output arrives as bytes. Lattice splits it on newlines and decodes each
line as UTF-8, and anything invalid becomes the replacement character, U+FFFD. A
compiler that emits a stray byte, or a tool that prints a filename in another
encoding, costs you that one character and nothing else.

Lattice used to drop the rest of that task's output at the first invalid byte.
The line that explains a failure usually comes after the noise that caused it, so
a failing task could report nothing at all.

A trailing `\r` goes with the newline, so output from a Windows tool does not
carry a `\r` into the display.

## Related pages

- [Run dev servers](/lattice/docs/dev-servers) for the run that forces raw mode
  and streams as it goes.
- [Run Lattice in CI](/lattice/docs/continuous-integration) for what a build
  machine gets and why.
- [Cache internals](/lattice/docs/cache-internals) for the component names in a
  `cache miss:` line.
- [CLI reference](/lattice/docs/cli) for every flag and how a flag, an
  environment variable, and a `settings` key rank against each other.
