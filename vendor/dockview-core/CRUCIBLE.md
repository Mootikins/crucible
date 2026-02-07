# dockview-core (Crucible Fork)

**Source:** https://github.com/mathuo/dockview
**Upstream version:** 4.13.1
**Reason:** Add "docked" as 4th location type for sliding side panels

## Patches Applied

(Initially empty — changes will be documented in subsequent tasks)

## Updating

To pull in upstream changes:

```bash
cd vendor/dockview-core
git init  # if needed
git remote add upstream https://github.com/mathuo/dockview
git fetch upstream
git diff upstream/master -- packages/dockview-core/src/
# Apply any new fixes manually, preserving our patches
```

## Build Configuration

The web package.json uses a file: dependency:

```json
"dockview-core": "file:../../../vendor/dockview-core"
```
