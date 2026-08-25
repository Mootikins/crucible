---
title: "cru project"
description: Manage the projects Crucible knows about
tags:
  - reference
  - cli
---

# cru project

Manage the projects Crucible knows about.

A **project** is where work output goes. A **kiln** is where knowledge goes. Crucible
keeps the two in separate registries with separate writers, so each one gets its own
command. See [[Help/CLI/kiln]] for the kiln side.

## Synopsis

```
cru project register [PATH]
cru project list
cru project forget <PATH>
```

## register

Register a directory as a project.

```bash
cru project register
cru project register ~/code/my-repo
```

The command registers the working directory when you name no path. The daemon resolves
a path inside a git repository to the repository root.

## list

Show the registered projects, by name and path.

```bash
cru project list
```

## forget

Remove one project registration.

```bash
cru project forget ~/code/my-repo
```

The directory itself is untouched. Only the registration goes.

## Where registrations live

The daemon owns the project registry, and it is the only writer. Registrations live in
`<data_home>/projects.json`, beside the kiln registrations in `kilns.json`. Neither file
is a config file: a registration is a fact the daemon was told, not a preference you
authored.

## See also

- [[Help/CLI/kiln]] — the same three verbs over the kiln registry
- [[Help/CLI/Index]] — every command
