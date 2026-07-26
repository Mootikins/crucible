# Hero artwork provenance

## Khunrath, *Alchemist's Laboratory* (c. 1595)

| | |
|---|---|
| **Files** | `khunrath-laboratory-{560,900,1600}.webp` |
| **Source work** | Plate from Heinrich Khunrath, *Amphitheatrum sapientiae aeternae* (Amphitheatre of Eternal Wisdom), c. 1595 |
| **After** | Hans Vredeman de Vries — the plate is signed *HF vriese pinxit*, i.e. he supplied the design; the engraver is not recorded on the source record |
| **License** | **Public domain** — published c. 1595; every contributor died more than three centuries ago, so it is PD worldwide by age |
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

1. **Measure the disc.** It is not centred and it is not round: the tondo sits
   1.5px right and 2px below the image centre, and is 15px taller than it is
   wide (rx 871.5, ry 886). Assuming a centred circle is what made the crop look
   subtly off. The scan is cropped to the measured ellipse, so the exported
   aspect is 1743:1772 and a CSS `border-radius: 50%` lands exactly on the rim.
2. Convert to luminance and autocontrast with a 0.5% cutoff on each tail — the
   scan is a faded hand-coloured impression, and the colour is discarded.
3. Apply gamma 1.25 and map to two tones: paper becomes amber `#F09A3E`, the
   burin lines become a warm near-black `#120B06`.

   Note the plate is **not** inverted. An earlier version was — light ink on a
   dark field — which read as a photographic negative. Keeping the ink dark on a
   lit disc reads as a struck medallion instead.
4. Mask to the measured ellipse with a hard edge: about 1.5px of antialiasing
   and no falloff. An earlier version dissolved over the outer 12% of the
   radius, which read as a vignette and ate the outer ring of the plate.
5. Composite onto `#0A0A0C` — the page background — and export opaque.

Opaque, not alpha: WebP encodes alpha losslessly, which tripled the file size.
The corners are cropped away in CSS anyway. 900px is 294 KB; the 1600px retina
variant is 817 KB.

Because the background is baked in, anything drawn *behind* the image is
occluded by its corners — the hero constellation runs behind it, so the image is
cropped with `border-radius: 50%` rather than left square.

