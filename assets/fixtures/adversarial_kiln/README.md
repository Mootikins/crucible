# Adversarial Retrieval Kiln

A 216-note synthetic corpus with five golden sets, each targeting one known
failure class of embedding-based note retrieval. Used with
`cru eval precognition --golden-dir assets/fixtures/adversarial_kiln/golden`
to localize which kind of miss a retrieval change fixes or breaks.

## Classes

- **vocabulary-mismatch.toml** (30 queries) — questions paraphrase hard; the
  expected notes use entirely different nouns ("screen goes dark when I game"
  → GPU Thermal Throttling). Isolates lexical-overlap dependence.
- **topic-dilution.toml** (25 queries) — sprawling multi-section compendium
  notes sit beside focused siblings on the same topic. The focused sibling is
  always the expected hit; isolates document-vector averaging dilution.
- **near-duplicate-interference.toml** (25 queries) — triplets share ~80%
  vocabulary and differ by one detail (beginner/pro, v1/v2, home/industrial).
  The query pins that one detail; isolates precision among near-ties.
- **cross-domain-confusion.toml** (25 queries) — confusable domain pairs
  (espresso/beer, investing/poker, medication/cooking) share vocabulary;
  the query must land in the right domain.
- **title-only-signal.toml** (20 queries) — thin bodies where titles carry the
  meaning; queries describe function without title words.

Plus ~90 filler notes across unrelated domains as realistic distractor mass.
All queries have exactly one defensible answer in the corpus (borderline ones
marked `lenient = true`, excluded from strict hit rate but counted in recall).

Class-level deltas between two runs are more trustworthy than absolute scores:
synthetic corpora carry their generator's biases.
