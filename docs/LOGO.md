# Solv logo

Status: **IMPLEMENTED** for the mark, the navbar lockup, and the favicon
(`frontend/src/components/Logo.tsx`, `frontend/index.html`). The export
checklist in §9 is outstanding.

This document extends `docs/DESIGN.md`, which had no logo guidance; its
colour, typography, and accessibility rules govern here too.

---

## 1. The idea

**A gate, seen head-on.** Two facing posts with an opening between them.

The product is a gate: something passes, or it does not. Every other
candidate metaphor described a by-product. A stamp describes the receipt, a
meter describes the score, a monogram describes nothing. The gate describes
what Solv is for, and it is the shape that survives being shrunk.

The mark carries a second reading that is a gift rather than an accident:
`[ ]`, a bracket pair. The audience puts this in front of a payment call in
their own code. A mark that looks at home in a terminal is telling the truth
about who it is for.

What the parts mean, if anyone asks:

| Part | Reading |
| --- | --- |
| Two posts | The two parties — the agent paying, the counterparty being paid |
| The opening | The decision. Solv does not move money; it decides whether money may move |
| Equal weight on both sides | The gate is neither the buyer's advocate nor the seller's. It reports |

---

## 2. Construction

A 24×24 grid in whole units. Six rectangles. No curves, no strokes, no
gradients, no rounded corners.

```
  0    6        14    24
0 ████████      ████████
  ██                  ██
  ██                  ██
  ██      gap 4       ██
  ██                  ██
  ██                  ██
18████████      ████████
24

block weight  6   (a quarter of the grid)
opening       4
arm reach    10   ((24 − 4) ÷ 2)
```

Every edge falls on a whole unit, so the mark lands on exact pixel
boundaries at 16, 24, 32, 48, and 64 px. `shape-rendering="crispEdges"`
stops the browser softening them.

### 2.1 The sealed variant

A 4×4 block centred in the opening: the decision, recorded. Display sizes
only — 32 px and above. Below that the block swallows the opening and the
gate stops reading as a gate.

Use it on the receipt page and in slide headers. Never in the navbar, never
as the favicon.

---

## 3. Colour

One colour at a time. The component paints with `currentColor`, so the mark
inherits whatever ink surrounds it and no separate dark-theme asset exists
to drift out of sync.

| Context | Mark | Field |
| --- | --- | --- |
| Navbar, light | Terracotta `#a8431f` | Warm Ledger `#f8f7f4` |
| Navbar, dark | Terracotta `#e8734a` | Obsidian `#12110f` |
| Favicon, app icon | Warm Ledger `#f8f7f4` | Terracotta `#a8431f` field |
| One colour only (print, stencil, laser) | Ink | Paper |

Never: a gradient, a second accent inside the mark, a drop shadow, or the
decision colours (`APPROVE`/`HOLD`/`BLOCK`). Those three belong to outcomes.
A logo that borrows green is claiming an outcome it does not have.

---

## 4. The lockup

Mark plus the word **Solv** in Space Grotesk Bold, tracking tight.

```
navbar     ▐█▌ Solv
footer     ▐█▌ Solv · pre-spend gate & counterparty registry
favicon    ▐█▌
```

- Gap between mark and wordmark: 0.6 × the mark's height.
- Wordmark cap height matches the mark's height; the type size is ~0.95 × the
  mark size, which lands the caps level by eye.
- The mark stands alone wherever the name already appears in text — favicons,
  avatars, slide corners.
- There is no drawn wordmark. "Solv" is set in the interface's own heading
  face, so the lockup cannot fall out of step with the product's typography.

**Clear space:** one block weight — a quarter of the mark's height — on all
four sides. Nothing crosses it, including the wordmark's descender box.

---

## 5. Sizes

| Size | Use | Variant |
| --- | --- | --- |
| 16 px | Favicon, browser tab | `open`, reversed on terracotta |
| 22 px | Navbar | `open` |
| 32–64 px | GitHub avatar, social card | `open`, reversed on terracotta |
| 96 px and up | Hero, slide titles, README header | `open` or `sealed` |

Minimum size is 16 px. Below that the 4-unit opening falls under one device
pixel and the mark becomes a filled square — at which point use a plain
terracotta square and accept that it is not the logo.

---

## 6. Misuse

Do not:

- Round the corners. The grid is the argument.
- Add an outline, a shadow, or a glow.
- Stretch either axis. The mark is square at every size.
- Rotate it. A gate stands upright.
- Recolour the opening, or place anything in it but the sealed block.
- Put the mark on a photograph, or on any field that is not a flat colour
  from the palette.
- Pair the wordmark with a face other than Space Grotesk.
- Animate the gate opening and closing in the navbar. Motion in the identity
  implies a live state the mark is not reporting (`docs/DESIGN.md` §5.1).

---

## 7. Why not the alternatives

Recorded so the question is not reopened from scratch later.

| Rejected | Reason |
| --- | --- |
| Stamp / seal | Describes the receipt, which is an output. Also crowded at 16 px once perforations are drawn |
| Band meter | Already exists in the UI as the band pips. A logo that repeats a component dilutes both |
| Monogram S | Says nothing about the product, and every B2B tool starting with S has one |
| Monoline 1 px | Matches the card borders beautifully at hero size and vanishes at 16 px |
| Monospace glyph `[\|]` | Kept as an informal text form for READMEs and commit messages; too fragile as an identity, since it renders differently in every font |

---

## 8. Text form

For plain-text contexts where an image cannot go — commit trailers, terminal
banners, issue templates:

```
[ ] solv
```

Informal. It is not the logo and carries no spacing or colour rules.

---

## 9. TODO

- [x] `SolvMark` and `SolvLockup` components, `open` and `sealed`
- [x] Navbar lockup
- [x] Favicon, reversed on terracotta, inline in `index.html`
- [ ] `frontend/public/solv-mark.svg` and `solv-lockup.svg` as standalone
      files, for the README and for anyone who needs the asset
- [ ] 512×512 PNG for the GitHub avatar
- [ ] Social card (1200×630) with the lockup and the one-sentence pitch
- [ ] `sealed` variant in the receipt page header, 48 px
- [ ] Link this document from `docs/DESIGN.md` §1
- [ ] Check the mark in both themes at 16 px on a low-contrast display before
      calling the identity done
