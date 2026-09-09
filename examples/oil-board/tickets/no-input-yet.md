---
title: An Oil input cannot send a value back
status: done
---

`oil.input` renders read-only in the browser: a view has no way to return a
typed value. Only `oil.action` crosses back. Decide whether an input needs its
own affordance or whether a view should ask through `cru.ui`.
