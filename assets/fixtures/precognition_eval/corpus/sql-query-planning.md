---
tags: [database, performance]
---
# SQL Query Planning

Read EXPLAIN QUERY PLAN before optimizing. Indexes turn table scans into lookups
but slow writes. Composite index column order matters: equality columns first,
range columns last.
