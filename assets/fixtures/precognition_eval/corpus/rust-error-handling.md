---
tags: [programming, rust]
---
# Rust Error Handling

Use Result<T, E> for recoverable failures and panic only for invariant breaks.
The question mark operator propagates errors upward. Libraries define error
enums; applications often use anyhow for ad-hoc context.
