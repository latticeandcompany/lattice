# Lattice Brand Guidelines


Lattice is a monochrome-first brand. Black and white carry the design; a single
accent color identifies each product. Keeping color rare is what makes it read as
meaningful when it appears.

---

## 1. Brand Architecture

There is one master brand — **Lattice** — and a family of products and entities
that share its type, layout, and construction. They differentiate through a
single assigned accent color, never through a different look.

### The core rule

**`Lattice` is always black or white. The product name takes the color.**

```
Lattice Build      →  "Lattice" in ink/paper, "Build" in teal
Lattice Cloud      →  "Lattice" in ink/paper, "Cloud" in purple
Lattice & Co.      →  fully neutral, no accent (the parent)
```

Use "Lattice Build" only alongside other products it needs to be distinguished
from. On its own, the lockup is black or white with no product word.

The wordmark `Lattice` never appears in an accent color. This keeps the parent
recognizable everywhere and lets products own their color without fragmenting
the brand.

### Entities & products

| Name            | Role                          | Accent            |
| --------------- | ----------------------------- | ----------------- |
| **Lattice & Co.** | Parent company                | Neutral (none)    |
| **Lattice Build** | The build system (this repo)  | Teal `#1B998B`    |
| **Lattice Cloud** | Cloud platform                | Purple `#6622CC`  |
| _Reserved_      | Future products               | Crimson `#CA2E55`, Orange `#FF6F00` |

A product keeps its color permanently. Crimson and orange are held in reserve;
do not spend them on marketing moments or one-off surfaces.

### Donated / independent projects

Some projects join the family while keeping their own brand (e.g. **ArenaSwap**).
They are **not** required to adopt Lattice colors, wordmark, or layout. The only
shared thread is **DM Sans** as the typeface, plus a family endorsement:

> Built with ❤️ by [Lattice](https://github.com/latticeandcompany)

placed in the footer. This signals lineage without absorbing the sub-brand.

---

## 2. Color

Primarily black and white. Accent color is the 5% — it appears on the product
name, primary CTAs, links, focus states, and the occasional key highlight.
Nothing else.

### Base (ink & paper)

| Token         | Hex       | Use                                   |
| ------------- | --------- | ------------------------------------- |
| `--ink`       | `#020D0C` | Near-black. Text on light, dark bg.   |
| `--paper`     | `#FBF8FF` | Near-white. Light bg, text on dark.   |

These are not pure `#000`/`#FFF`. They are deeply darkened / lightened versions
of the accent palette, so black and white feel like they belong to the same
world as the colors. Never substitute `#000000` or `#FFFFFF`.

### Neutrals (derived gray ramp)

Grays are mixes of `--ink` and `--paper`. Use for surfaces, borders, secondary
text, and disabled states.

| Token         | Hex       | Use                          |
| ------------- | --------- | ---------------------------- |
| `--gray-900`  | `#141F1E` | Elevated dark surface        |
| `--gray-700`  | `#3A4443` | Muted text on light          |
| `--gray-500`  | `#6E7776` | Secondary text, icons        |
| `--gray-300`  | `#B6BCBB` | Borders, dividers            |
| `--gray-100`  | `#E7E6E9` | Subtle fills, hover surfaces |

### Accents

Each accent ships with a tint (`-300`), base (`-500`), and shade (`-700`) for
hover/active and light/dark contexts. Use `-500` as the canonical value.

**Teal — Lattice Build**
| Token        | Hex       |
| ------------ | --------- |
| `teal-300`   | `#63C4B8` |
| `teal-500`   | `#1B998B` |
| `teal-700`   | `#136E64` |

**Purple — Lattice Cloud**
| Token          | Hex       |
| -------------- | --------- |
| `purple-300`   | `#9A66E0` |
| `purple-500`   | `#6622CC` |
| `purple-700`   | `#4A1795` |

**Crimson — reserved**
| Token           | Hex       |
| --------------- | --------- |
| `crimson-300`   | `#E06A87` |
| `crimson-500`   | `#CA2E55` |
| `crimson-700`   | `#92203D` |

**Orange — reserved**
| Token          | Hex       |
| -------------- | --------- |
| `orange-300`   | `#FF9E4D` |
| `orange-500`   | `#FF6F00` |
| `orange-700`   | `#B84F00` |

### The 60 · 25 · 10 · 5 rule

Every surface should budget color roughly this way:

- **60 %** — dominant base (`--paper` on light UI, `--ink` on dark UI)
- **25 %** — secondary surface / body text (the opposite base + `gray-900/100`)
- **10 %** — neutral structure (grays: borders, muted text, icons)
- **5 %** — the product accent (colored product name, primary CTA, links, focus)

If the accent creeps past ~5 % of a view, pull it back. Restraint reads as
confidence.

### Terminal label colors — the one exception

`lattice run`'s plain stream colors the `workspace:task` label at the head of
each line, one color per task, from a palette of eight hues one 45° step apart
around the wheel (`LABEL_PALETTE` in `crates/lattice-output`). Those hues are
not brand colors and are not accents.

This is the only surface where color is a functional index rather than an
accent. A parallel run interleaves lines from every task at once, and the color
is the only thing that lets a reader follow one of them — there is no layout to
do it with. Reusing teal would mean every label the same color, which indexes
nothing; spending crimson and orange here would burn two reserved product
accents on a log.

It does not license a second palette anywhere else. Interactive mode, the site,
the docs, and every other surface stay on §2 as written, and even here the rest
of the line is unstyled: nothing but the label takes a color.

### Accessibility

- Body text must clear **WCAG AA** (4.5:1). `--ink` on `--paper` passes with
  wide margin.
- Accent-on-base is for large text, CTAs, and non-text UI only. Do not set long
  body copy in an accent color. `teal-500`/`purple-500` on `--paper` pass AA for
  large/bold text; verify per-context.
- Never rely on color alone to convey meaning.

---

## 3. Typography

One family across the entire brand — including donated projects.

### Typefaces

- **DM Sans** — everything: UI, marketing, headings, body. Geometric-humanist,
  clean, low-key. The single shared thread across all Lattice properties.
- **DM Mono** — code, terminal output, version numbers, technical metadata,
  keyboard shortcuts. Same family lineage, sits naturally beside DM Sans.

No third typeface. DM Sans and DM Mono only.

### Weights

- **Regular (400)** — body copy
- **Medium (500)** — UI labels, emphasis, small headings
- **Bold (700)** — display headings, wordmark

Avoid lighter-than-400 weights; they weaken the confident tone and hurt
legibility at small sizes.

### Scale (type ramp)

A modular scale (~1.25). Adjust per surface, but keep the ratio.

| Token     | Size    | Weight | Use                    |
| --------- | ------- | ------ | ---------------------- |
| `display` | 60 / 64 | 700    | Hero headlines         |
| `h1`      | 40 / 48 | 700    | Page titles            |
| `h2`      | 32 / 40 | 700    | Section headers        |
| `h3`      | 24 / 32 | 500    | Subsections            |
| `body-lg` | 18 / 28 | 400    | Lead paragraphs        |
| `body`    | 16 / 24 | 400    | Default body           |
| `small`   | 14 / 20 | 400    | Captions, metadata     |
| `mono`    | 14 / 22 | 400    | Code, DM Mono          |

### Setting type

- Tight, deliberate tracking on display (`-0.02em`); default elsewhere.
- Generous line-height on body (1.5); tighter on headings (1.1–1.2).
- Left-aligned. Avoid justified or centered body copy.

---

## 4. Voice & Tone

**Precise and confident.** Terse, technical, understated. Vercel / Linear
register, never hype. This applies to every string we write: marketing copy, docs
prose, CLI help, error messages, and code comments.

### The official tagline is exempt

**A high-performance, local toolchain for managing monorepos.**

This string is fixed in `MESSAGING.md` and overrides everything below. It uses a
performance adjective and names local execution on purpose. Do not rewrite it to
satisfy the rules in this section, and do not flag it in a copy review. Every
*other* string follows the rules that follow.

### Don't sell the pillars

Local execution, interop with other task runners, and leaving tool choice to the
developer are core to the project. Outside the tagline, do not write copy about
them. They are expected, not achievements, and naming them is the fastest way to
sound like we are compensating for something.

Concretely, these phrases do not ship: *local-first*, *never prescribe*, *declare
or detect*, *zero-config*, *nothing global*, *no account required*, *never
guesses*. Neither does a value prop, heading, or feature bullet named after one.

Describe the benefit instead. "The cache is on your disk, so nothing leaves your
machine" is three restatements of a pillar; "Change one file and only what depends
on it runs again" is the thing the user actually wanted.

### Never name another task runner in prose

We are inspired by the tools in this space and we interoperate with them. We do
not position against them by name. Copy describes the capability — "a project that
already has its own task runner becomes one workspace" — and never the competitor.
A real tool name inside a code sample is fine, because a fake command in a config
example is worse.

### Banned words

*polyglot* — say "any language" or name the languages.

### We are

- **Direct.** Short sentences. Say the thing. "Builds in parallel. Caches
  everything."
- **Technical without jargon-for-its-own-sake.** Precise words, no filler.
- **Understated.** Claims are measurable, not superlative. "Fast" is shown, not
  shouted.

### We avoid

- Hype and exclamation ("🚀 Blazingly fast revolutionary!!!").
- Hedging ("we think maybe this could possibly help").
- Marketing throat-clearing ("In today's fast-paced world...").
- **Unmeasurable comparatives.** "Builds finish sooner" and "spend less time
  waiting" name no baseline. Say what the mechanism does. (The tagline's
  "high-performance" is the sanctioned exception.)
- **Invented numbers.** We have no published benchmarks. Never write a figure we
  have not measured — not in copy, not as an illustration.
- **The bolded-lead-in bullet list.** `**Thing** — explanation`, repeated six
  times under a "## Features" heading, is the house tic. One list per document at
  most; prefer prose.
- **Rhythmic triads.** "Fast, simple, and reliable." "No detection, no plugin, no
  engines." The third item is almost always there for the cadence.
- **The em-dash dramatic pause.** Use it to bracket a genuine aside, not to land a
  beat. If the sentence works with a period, use the period.
- **Aphorisms about ourselves.** "The discipline is the point." "Space is a
  feature." "That split is the whole trade." Cut them.

### Examples

| Instead of | Write |
| --- | --- |
| "The most powerful build tool ever created!" | "A high-performance, local toolchain for managing monorepos." |
| "We're super excited to announce…" | "New: remote caching in Lattice Cloud." |
| "It's really really fast" | "Runs tasks in parallel and caches results by content." |
| "Blazingly fast builds." | "Change one file and only what depends on it runs again." |
| "Local-first and zero-config." | "Reads the tools already in each directory." |
| "Builds that finish sooner." | "Unchanged work comes back from cache." |

The ❤️ in the endorsement footer is the one sanctioned moment of warmth. It is a
nod to the community, not a shift in register.

---

## 5. Layout & Feel

Vercel-inspired: high contrast, generous whitespace, sharp structure.

Give elements room. Spacing is deliberate, not incidental.

Depth is sharp and subtle: thin `gray-300` borders rather than heavy shadows,
radii small and consistent (4–8px), elevation from a surface shift rather than a
drop shadow.

Screens read black-and-white at a glance, with the accent as the one thing the eye
lands on.

Dark and light both ship. `--ink` and `--paper` swap cleanly, and the accent holds
its meaning in both.

---

## 6. Logo & Wordmark

The Lattice mark is a **woven-sphere rosette** — internally, the pie, specifically
apple pie. Eight overlapping ellipses rotated evenly around a shared center form
an interlaced lattice with a clean polygonal aperture at its core. It reads as a
dependency graph and as a woven structure.

### Assets

All paths are relative to `marketing/`.

| File                    | What it is                       | Use on                   |
| ----------------------- | -------------------------------- | ------------------------ |
| `icon-black.svg`        | Standalone mark, ink             | Light backgrounds        |
| `icon-white.svg`        | Standalone mark, paper           | Dark backgrounds         |
| `lockup-black.svg`      | Mark + wordmark, ink             | Light backgrounds        |
| `lockup-white.svg`      | Mark + wordmark, paper           | Dark backgrounds         |
| `icon-black-small.svg`  | Small-size mark, ink             | ≤ 48px, light bg         |
| `icon-white-small.svg`  | Small-size mark, paper           | ≤ 48px, dark bg          |
| `favicon.svg`           | Simplified 4-ring mark, adaptive | Favicon (auto ink/paper) |
| `favicon-black.svg`     | Simplified 4-ring mark, ink      | Favicon, light bg        |
| `favicon-white.svg`     | Simplified 4-ring mark, paper    | Favicon, dark bg         |
| `pattern.svg`           | Repeating rosette motif          | Background bands         |
| `ascii-art.txt`         | Terminal rosette                 | CLI output               |
| `ascii-art-full.txt`    | Terminal rosette, full detail    | CLI splash               |

- **Icon** — app icons, favicons, avatars, tight spaces, or as a repeating
  brand motif. Never pair the icon with separately-typed text; use the lockup.
- **Lockup** — the default brand signature: navbars, docs headers, README,
  footers, slides.

`lattice-and-co/` holds the parent-company marks — a horizontal and a stacked
lockup, plus an `L&Co` monogram, each in a black-on-white and a white-on-black
export. These are set in a serif, not DM Sans, and they are never substituted for
a Lattice product mark.

### Where each copy lives

The same mark exists in three places, and each one is load-bearing:

| Location                  | Which copy                     | Why it exists                             |
| ------------------------- | ------------------------------ | ----------------------------------------- |
| `marketing/`              | The editable source            | Wordmark is live `<text>` in DM Sans       |
| `apps/web/public/brand/`  | Outlined, served by the site   | Astro can only serve from `public/`        |
| `.github/assets/`         | Outlined, rendered in the README | GitHub resolves README paths from `.github/` |

Edit `marketing/` first, then re-export the outlined copies. The site and README
versions have the wordmark converted to paths so they render without DM Sans
installed — do not overwrite them with the source SVG.

### The wordmark

Set in **DM Sans Medium (500)**, always **lowercase** — `lattice`. The
lowercase wordmark is a fixed stylization (à la `vercel`, `stripe`); it does not
change with sentence case. In running prose the product is still capitalized
(“Lattice builds in parallel”). Only the rendered wordmark is lowercase.

Do not re-typeset the wordmark by hand — use the lockup SVG so kerning and the
mark-to-text gap stay locked.

### Applying the product-color rule to the lockup

Per §1, `lattice` in the wordmark stays ink or paper — never an accent. Product
identity attaches to the **product word set beside it**, not to `lattice` and
not to the mark:

```
[rosette]  lattice  build      ← "build" in teal-500
[rosette]  lattice  cloud      ← "cloud" in purple-500
[rosette]  lattice             ← parent / Lattice & Co., no accent
```

The **mark itself is always monochrome** (ink or paper). It may sit on an
accent-colored surface, but its strokes do not take an accent.

### Clear space & minimum size

- **Clear space:** keep padding on all sides equal to the diameter of the mark’s
  center aperture (≈ ¼ of the mark’s height). Nothing intrudes.
- **Minimum size — lockup:** 120px wide (screen) / 24px tall mark. Below this,
  switch to the standalone icon.
- **Minimum size — icon:** the standard mark holds down to ~48px. **At 48px and
  below, switch to the small variant** (`icon-*-small.svg`): a
  thinner stroke, cropped tight to the rosette. The thin stroke keeps the woven
  gaps open where the standard mark's heavier strokes blur into a blob. Below
  ~20px (16px favicons) even the small mark crowds — use **`favicon.svg`**, a
  simplified 4-ring mark with an open aperture that stays legible at 16px. It is
  theme-adaptive (ink on light browser chrome, paper on dark).

### Backgrounds

- Prefer ink-on-paper or paper-on-ink.
- On a photo or busy surface, place the lockup on a solid ink/paper chip with
  clear space, not directly on the image.
- On an accent-colored surface, use the **paper** (white) mark for contrast —
  never a second accent.

### Don’ts

- Don’t recolor the mark’s strokes to an accent, gradient, or non-brand color.
- Don’t rotate, shear, or squash the mark (its symmetry is the point).
- Don’t add effects — shadows, bevels, outlines, glows.
- Don’t change the mark-to-wordmark spacing or re-typeset the wordmark.
- Don’t place the ink mark on a dark surface (or paper on light) — keep contrast.
- Don’t use pure `#000`/`#FFF`; the mark uses `--ink` / `--paper`.

---

## 7. Endorsement Footer (donated projects)

For independent projects in the family (e.g. ArenaSwap), include in the footer:

```html
<footer>
  Built with ❤️ by <a href="https://github.com/latticeandcompany">Lattice</a>
  · <a href="https://github.com/latticeandcompany">Join our GitHub org</a>
</footer>
```

- Set in **DM Sans** (the shared thread).
- Uses the host project's own colors, not Lattice's accents.
- Keep it small and quiet — a signature, not a banner.

---

## Quick reference

- `Lattice` = always ink or paper. Product name = the color.
- Base: `--ink #020D0C`, `--paper #FBF8FF` — never pure black/white.
- Build = teal `#1B998B` · Cloud = purple `#6622CC` · Co. = neutral · crimson &
  orange reserved.
- Type: **DM Sans** everywhere, **DM Mono** for code.
- 60 · 25 · 10 · 5 — accent is the 5 %.
- Voice: precise, confident, understated.
