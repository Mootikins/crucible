---
title: Byte spans survive a rename
status: doing
---

The parser hands back byte spans. A rename splices the file, so every span
after the edit shifts. Check that the link index re-reads rather than adjusts.
