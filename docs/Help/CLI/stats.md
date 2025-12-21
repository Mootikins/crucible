---
description: Display statistics about your kiln
tags:
  - reference
  - cli
---

# cru stats

Display summary statistics about your kiln directory.

## Synopsis

```
cru stats
```

## Description

The `stats` command scans your kiln directory and provides a summary of its contents. It recursively walks through all subdirectories and reports file counts, markdown file counts, and total storage size.

This command is useful for:
- Getting a quick overview of your kiln's size
- Monitoring growth over time
- Verifying that your kiln path is configured correctly

## Options

The `stats` command currently takes no options or flags. It operates on the kiln path configured in your Crucible configuration file.

## Statistics Reported

### Total Files
The total number of files in your kiln directory and all subdirectories.

### Markdown Files
The count of files with a `.md` extension (case-insensitive).

### Total Size
The combined size of all files in your kiln, reported in kilobytes (KB).

### Kiln Path
The absolute path to your kiln directory.

## Example Output

```
Kiln Statistics

Total files: 127
Markdown files: 89
Total size: 2048 KB
Kiln path: /home/user/my-kiln

Kiln scan completed successfully.
```

## Error Conditions

### Kiln Path Not Configured

```
Error: kiln path does not exist: /path/to/nonexistent
Please configure kiln.path in your config file (see: cru config show)
```

**Solution**: Configure your kiln path:

```toml
[kiln]
path = "/path/to/your/kiln"
```

### Permission Errors

If the command cannot read certain directories or files, those items will be skipped and the scan will continue.

## Implementation Details

The stats command:
- Recursively scans all subdirectories
- Identifies markdown files by `.md` extension (case-insensitive)
- Uses filesystem metadata for file sizes
- Uses saturating addition to prevent overflow on very large kilns

## Source Code

**Implementation:** `crates/crucible-cli/src/commands/stats.rs`

## See Also

- `:h process` - Process your kiln for search and queries
- `:h search` - Search indexed content
- `:h config.kiln` - Kiln configuration options
