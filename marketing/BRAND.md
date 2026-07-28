# Lattice Brand Guidelines


Lattice is a monochrome-first brand. Black and white do the heavy lifting; a
single accent color identifies each product. The discipline is the point — when
color is rare, it means something.

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

**IMPORTANT TO NOTE**: lattice build should only be used when in the presence of other products that it needs to be differentiated from. If lattice build is on its own it should only be a black or white lockup.

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

One product, one color, for life. Crimson and orange are held in reserve — do
not spend them on marketing moments or one-off surfaces.

### Donated / independent projects

Some projects join the family while keeping their own brand (e.g. **ArenaSwap**).
They are **not** required to adopt Lattice colors, wordmark, or layout. The only
shared thread is **DM Sans** as the typeface, plus a family endorsement:

> Built with ❤️ by [Lattice](https://github.com/<org>)

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

No third typeface. If it isn't DM Sans or DM Mono, it doesn't ship.

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

**Precise and confident.** Terse, technical, understated. We let the product
speak and trust the reader to be smart. Vercel / Linear register — never hype.

Important note:
While local first, turborepo interop, devloper choice are all core pillars of this project, we try to avoid talking about them. They are expected, not something to brag about

**We are:**

- **Direct.** Short sentences. Say the thing. "Builds in parallel. Caches
  everything."
- **Technical without jargon-for-its-own-sake.** Precise words, no filler.
- **Understated.** Claims are measurable, not superlative. "Fast" is shown, not
  shouted.

**We avoid:**

- Hype and exclamation ("🚀 Blazingly fast revolutionary!!!").
- Hedging ("we think maybe this could possibly help").
- Marketing throat-clearing ("In today's fast-paced world...").

**Examples**

| Instead of                                   | Write                          |
| -------------------------------------------- | ------------------------------ |
| "The most powerful build tool ever created!" | "A build system for polyglot monorepos." |
| "We're super excited to announce…"          | "New: remote caching in Lattice Cloud." |
| "It's really really fast"                    | "Cold builds in 4s. Cached in 90ms." |

The ❤️ in the endorsement footer is the one sanctioned moment of warmth — a nod
to the community, not a shift in register.

---

## 5. Layout & Feel

Vercel-inspired: high contrast, generous whitespace, sharp structure.

- **Space is a feature.** Let elements breathe. Whitespace signals confidence.
- **Sharp, subtle depth.** Thin `gray-300` borders over heavy shadows. Radii
  small and consistent (4–8px). Elevation via subtle surface shifts, not drop
  shadows.
- **Monochrome dominant.** Screens read black-and-white at a glance; the accent
  is the one thing your eye lands on.
- **Dark and light both first-class.** `--ink` and `--paper` swap cleanly; the
  accent holds its meaning in both.

---

## 6. Logo & Wordmark

The Lattice mark is a **woven-sphere rosette** (we call it the pie (specifically apple pie)) — eight overlapping ellipses
rotated evenly around a shared center, forming an interlaced lattice with a
clean polygonal aperture at its core. It reads at once as a dependency graph, a
woven structure, and a node — exactly what a build system is.

### Assets

| File                             | What it is                    | Use on            |
| -------------------------------- | ----------------------------- | ----------------- |
| `lattice_icon_black.svg`         | Standalone mark, ink          | Light backgrounds |
| `lattice_icon_white.svg`         | Standalone mark, paper        | Dark backgrounds  |
| `lattice_icon_black_lockup.svg`  | Mark + wordmark, ink          | Light backgrounds |
| `lattice_icon_white_lockup.svg`  | Mark + wordmark, paper        | Dark backgrounds  |
| `lattice_icon_black_small.svg`   | Small-size mark, ink          | ≤ 48px, light bg  |
| `lattice_icon_white_small.svg`   | Small-size mark, paper        | ≤ 48px, dark bg   |
| `favicon.svg`                    | Simplified 4-ring mark, adaptive | Favicon (auto ink/paper) |
| `favicon_black.svg`              | Simplified 4-ring mark, ink   | Favicon, light bg |
| `favicon_white.svg`              | Simplified 4-ring mark, paper | Favicon, dark bg  |

- **Icon** — app icons, favicons, avatars, tight spaces, or as a repeating
  brand motif. Never pair the icon with separately-typed text; use the lockup.
- **Lockup** — the default brand signature: navbars, docs headers, README,
  footers, slides.

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
  below, switch to the `_small` variant** (`lattice_icon_*_small.svg`): a
  thinner stroke, cropped tight to the rosette. The thin stroke keeps the woven
  gaps open where the standard mark's heavier strokes blur into a blob. Below
  ~20px (16px favicons) even the `_small` mark crowds — use **`favicon.svg`**, a
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
  Built with ❤️ by <a href="https://github.com/<org>">Lattice</a>
  · <a href="https://github.com/<org>">Join our GitHub org</a>
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
