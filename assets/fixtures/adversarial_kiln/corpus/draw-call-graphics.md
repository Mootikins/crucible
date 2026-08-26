---
tags: [programming]
---
# Draw Call Budget

A draw call submits geometry to the GPU. Budget them per frame; exceeding the cap stalls the render thread and drops framerate before the GPU itself is saturated.
