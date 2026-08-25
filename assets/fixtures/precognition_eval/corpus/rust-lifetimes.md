---
tags: [programming, rust]
---
# Rust Lifetimes

A lifetime describes how long a reference stays valid. Borrow checker rejects
a reference that outlives its owner. Explicit annotations start at 'a and are
only needed when the compiler cannot infer them.
