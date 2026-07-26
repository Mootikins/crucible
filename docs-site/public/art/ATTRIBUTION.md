# Hero artwork provenance

## Khunrath, *Alchemist's Laboratory* (c. 1595)

| | |
|---|---|
| **Files** | `khunrath-laboratory-{560,900,1600}.webp` |
| **Source work** | Plate from Heinrich Khunrath, *Amphitheatrum sapientiae aeternae* (Amphitheatre of Eternal Wisdom), c. 1595 |
| **Engraver** | Hans Vredeman de Vries |
| **License** | **Public domain** — published c. 1595, author died 1607; PD worldwide by age |
| **Source file** | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Alchemist%27s_Laboratory,_Heinrich_Khunrath,_Amphitheatrum_sapientiae_aeternae,_1595_c.jpg) |
| **Original scan** | 1750 × 1782, hand-coloured impression |

No attribution is legally required. It is recorded here anyway so the provenance
of the site's most prominent asset is not folklore.

### Why this plate

The engraving is a single room split between an *oratorium* (tent, books, a
kneeling figure) on the left and a *laboratorium* (furnace, glassware, apparatus)
on the right, with instruments laid across the table between them. Knowledge on
one side, the apparatus that acts on it on the other — which is the same claim
the site makes in prose.

### Derivation

Generated from the Commons scan. The steps, so they can be reproduced or retuned:

1. Convert to luminance, autocontrast with 0.5% cutoff on each tail (the scan is
   a faded hand-coloured impression; the colour is discarded).
2. **Invert.** The original is dark ink on light paper; the site is light ink on
   a dark field. Inversion is what makes the plate usable here at all.
3. Apply gamma 1.45, then map to an amber ramp from `#5C2A0D` (faint ink) to
   `#FFAA33` (dense ink), with the densest 28% pushed toward white-gold.
4. Mask to the tondo with a hard edge — about 1.5px of antialiasing and no
   falloff. An earlier version dissolved over the outer 12% of the radius,
   which read as a vignette rather than a struck medallion and ate the outer
   ring of the plate along with it.
5. Composite onto `#0A0A0C` — the page background — and export opaque.

Opaque, not alpha: WebP encodes alpha losslessly, which tripled the file size.
The disc's corners are the page colour, so on the page they are invisible
anyway. 900px is 281 KB; the 1600px retina variant is 816 KB.

One consequence of baking the background in: anything drawn *behind* the image
is occluded by those corners. A radial glow underneath the medallion showed up
as a visible square, which is why there isn't one.

Steps 3 and 5 hardcode the palette. If the page background or the amber changes,
these files must be regenerated — they will not adapt.
