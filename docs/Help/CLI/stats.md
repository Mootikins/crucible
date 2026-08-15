---
title: "cru stats"
description: Display statistics about your kiln
tags:
  - reference
  - cli
---

# cru stats

Display summary statistics about your kiln directory.

## Synopsis

```
cru stats [-f <format>]
```

## Description

The `stats` command scans your kiln directory and provides a summary of its contents. It recursively walks through all subdirectories and reports file counts per kind, how many files the kiln indexes, and total storage size.

This command is useful for:
- Getting a quick overview of your kiln's size
- Monitoring growth over time
- Verifying that your kiln path is configured correctly

## Options

### `-f, --format <format>`

`text` (default) or `json`. `table` and `plain` are accepted as aliases for
`text`. There is no table rendering: the report is four counts and a path, which
a table would only make wider.

Otherwise `stats` takes no flags. It operates on the kiln path configured in your
Crucible configuration file.

## Statistics Reported

### Total Files
The total number of files in your kiln directory and all subdirectories.

### Markdown Files
The count of `.md` and `.markdown` notes (case-insensitive).

### Canvases
The count of `.canvas` documents. Only shown when the kiln contains at least one.

### Plain Text
The count of `.txt` files. Only shown when the kiln contains at least one.

### Indexed
Markdown notes plus canvases plus plain text — every file the kiln indexes, and
the number `cru process` reports as discovered. The three kinds are counted
separately because they are not interchangeable: notes and canvases join the link
graph, plain text is only full-text searchable.

### Total Size
The combined size of all files in your kiln, reported in kilobytes (KB).

### Kiln Path
The absolute path to your kiln directory.

## Example Output

```
📊 Kiln Statistics

📁 Total files: 127
📝 Markdown files: 89
🎨 Canvases: 2
📄 Plain text: 4
🔍 Indexed: 95
💾 Total size: 2048 KB
🗂️  Kiln path: /home/user/my-kiln

✅ Kiln scan completed successfully.
```

Total files counts everything on disk, including images and attachments the kiln
does not index — so `Total files` is normally larger than `Indexed`.

## Error Conditions

### Kiln Path Not Configured

```
Error: kiln path does not exist: /path/to/nonexistent
Please configure kiln.path in your config file (see: cru config show)
```

**Solution**: Configure your kiln path:

```toml
default_kiln = "main"

[kilns]
main = "/path/to/your/kiln"
```

### Permission Errors

If the command cannot read certain directories or files, those items will be skipped and the scan will continue.

## Implementation Details

The stats command:
- Recursively scans all subdirectories
- Classifies each file with the same predicate the indexer uses, so `cru stats`
  and `cru process` cannot disagree about what counts
- Uses filesystem metadata for file sizes
- Uses saturating addition to prevent overflow on very large kilns

## See Also

- `:h process` - Process your kiln for search and queries
- `:h search` - Search indexed content
- `:h config.kiln` - Kiln configuration options
