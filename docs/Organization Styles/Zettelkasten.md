---
description: Atomic linked notes for building interconnected knowledge
tags:
  - organization
  - zettelkasten
  - research
---

# Zettelkasten

Zettelkasten (German for "slip box") is a note-taking method focused on small, atomic notes that link densely to each other. Structure emerges organically from connections rather than predetermined hierarchies.

## Core Principles

### Atomic Notes

Each note contains **one idea**. If a note covers multiple ideas, split it.

**Good:**
```markdown
# Compound Interest

Compound interest is interest calculated on both the initial principal
and accumulated interest from previous periods.

The formula is: A = P(1 + r/n)^(nt)

Related: [[Time Value of Money]], [[Investment Growth]]
```

**Too broad:**
```markdown
# Finance Concepts
[Multiple unrelated concepts in one note]
```

### Dense Linking

Every note should link to related notes. Links are the structure.

```markdown
# Spaced Repetition

Spaced repetition is a learning technique that reviews material at
increasing intervals to optimize retention.

It leverages the [[Forgetting Curve]] by timing reviews just before
information would be forgotten.

This connects to [[Active Recall]] and [[Interleaving]], which also
improve learning efficiency.

See also:
- [[Anki]] - software implementing spaced repetition
- [[Learning Techniques Index]] - overview of methods
```

### Emergent Structure

Don't create categories upfront. Let structure emerge from links.

As you write, patterns appear:
- Clusters of related notes
- Hub notes that link to many
- Linear sequences of thought

## Note Types

### Fleeting Notes

Quick captures, unprocessed thoughts:
```markdown
# 2024-01-15 Fleeting

- Idea about connecting productivity with creativity
- Look up research on incubation periods
- Talk to Sarah about project timeline
```

Process these into permanent notes or discard.

### Literature Notes

Summaries of sources in your own words:
```markdown
# Deep Work by Cal Newport

Core thesis: Focused, distraction-free work produces valuable results.

Key concepts:
- [[Deep Work]] vs shallow work
- [[Attention Residue]] harms productivity
- [[Deliberate Practice]] requires depth

My thoughts: This connects to my notes on [[Flow States]].
```

### Permanent Notes

Refined, atomic ideas:
```markdown
# Attention Residue

When switching tasks, cognitive resources remain partially allocated
to the previous task. This reduces performance on the new task.

Source: Sophie Leroy's research on task switching.

Implications:
- Batch similar tasks together
- Complete tasks before switching when possible
- Use [[Time Blocking]] to protect focus periods

Related:
- [[Deep Work]] - argues for extended focus
- [[Multitasking Myth]] - why it doesn't work
```

## Zettelkasten in Crucible

### File Naming

Two common approaches:

**Title-based (recommended for Crucible):**
```
Attention Residue.md
Deep Work.md
```

**ID-based (traditional Zettelkasten):**
```
202401151423 Attention Residue.md
202401151445 Deep Work.md
```

### Frontmatter

```yaml
---
type: permanent  # fleeting, literature, permanent
source: Sophie Leroy research
created: 2024-01-15
tags:
  - psychology
  - productivity
---
```

### Creating Connections

Use wikilinks liberally:
```markdown
This idea connects to [[Related Concept]] because...

See also:
- [[Connection 1]] - explanation
- [[Connection 2]] - explanation
```

### Structure Notes

Create hub notes that link clusters:
```markdown
# Productivity MOC

## Core Concepts
- [[Deep Work]]
- [[Attention Residue]]
- [[Flow States]]

## Techniques
- [[Time Blocking]]
- [[Pomodoro Technique]]
- [[Batching]]

## Resources
- [[Deep Work by Cal Newport]]
- [[Flow by Mihaly Csikszentmihalyi]]
```

## Building a Zettelkasten

### Start With Reading

1. Read actively with questions in mind
2. Take literature notes in your words
3. Extract permanent notes from insights
4. Link to existing notes

### Daily Practice

1. Process fleeting notes
2. Add links to new notes
3. Revisit and strengthen connections
4. Let structure emerge

### Review and Refine

- Follow chains of links
- Discover unexpected connections
- Split notes that grew too large
- Combine notes that cover same idea

## Tradeoffs

**Strengths:**
- Flexible, adapts to any domain
- Discovers unexpected connections
- Builds compounding knowledge
- No category decisions needed

**Challenges:**
- Requires writing discipline
- Navigation can be challenging at first
- Benefits emerge over time
- Works best for knowledge work

## When to Use Zettelkasten

Zettelkasten works best when you:
- Focus on learning and research
- Want ideas to connect organically
- Build knowledge over long periods
- Prefer bottom-up organization
- Write regularly

## Combining with Other Methods

Zettelkasten combines well with:
- **PARA** - Use Zettelkasten within Resources
- **MOCs** - Hub notes provide navigation
- **Johnny Decimal** - For stable reference alongside atomic notes

## See Also

- [[Index]] - Overview of organization styles
- [[PARA]] - Organizing by actionability
- [[Choosing Your Structure]] - Help deciding which method to use
- `:h wikilinks` - Linking syntax
