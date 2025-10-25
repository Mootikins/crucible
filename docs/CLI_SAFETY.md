# 🔒 Crucible CLI Safety Features

> Comprehensive guide to memory protection, input validation, and error handling in Crucible CLI

## Overview

Crucible CLI includes built-in safety protections to ensure reliable performance and prevent system resource issues. These protections work automatically in the background while providing helpful error messages when validation fails.

## 🛡️ Memory Protection

### File Size Limits

| Protection | Limit | Behavior | Error Message |
|------------|-------|----------|---------------|
| **Maximum File Size** | 10MB | Files larger than 10MB are automatically skipped | `File too large (16MB > 10MB limit): path/to/file.md` |
| **Content Memory Limit** | 1MB | Large files processed with streaming reads to enforce memory limit | `File content exceeds memory limit (1MB): path/to/file.md` |

### Streaming File Processing

- **8KB Buffers**: Large files read in 8KB chunks to manage memory efficiently
- **Memory Monitoring**: Tracks total bytes read to prevent memory exhaustion
- **Graceful Degradation**: Continues processing other files if one file is too large

**Example**:
```bash
# Large file automatically skipped
crucible-cli search "query"  # in vault with 50MB file
# Output: Files processed successfully, large file skipped with warning
```

## 🔤 UTF-8 Safety

### Encoding Error Recovery

- **Automatic Detection**: Detects UTF-8 encoding errors during file reading
- **Character Replacement**: Invalid UTF-8 sequences replaced with Unicode replacement character (�)
- **Continuation**: Processing continues even with corrupted files
- **International Support**: Full support for international text, emoji, and special characters

**Error Handling Examples**:
```bash
# File with invalid UTF-8 (handled gracefully)
crucible-cli search "café"  # in file with encoding issues
# Result: Search continues, invalid characters replaced safely
```

### Supported Character Sets

- ✅ **Unicode**: All Unicode characters including emoji (🚀, 📊, 🔍)
- ✅ **International**: Accented characters (café, résumé, naïve)
- ✅ **Special Symbols**: Mathematical symbols, currency symbols, technical symbols
- ✅ **Mixed Scripts**: Combinations of Latin, Cyrillic, Chinese, Arabic, etc.

## ✅ Input Validation

### Search Query Validation

| Validation Rule | Requirement | Error Message |
|------------------|-------------|---------------|
| **Minimum Length** | 2 characters | `Search query too short (1 < 2 characters). Please provide a more specific query.` |
| **Maximum Length** | 1000 characters | `Search query too long (1001 > 1000 characters). Please use a shorter query.` |
| **Empty Queries** | Non-empty after trim | `Search query cannot be empty or only whitespace.` |
| **Null Characters** | No null bytes | `Search query contains invalid null characters.` |
| **Whitespace** | Normalized automatically | Multiple spaces collapsed to single space |

### Validation Examples

```bash
# ✅ Valid queries
crucible-cli search "machine learning"
crucible-cli search "café"
crucible-cli search "🚀 project planning"
crucible-cli search "research notes"

# ❌ Invalid queries (show helpful errors)
crucible-cli search ""                    # Empty query
crucible-cli search "a"                   # Too short
crucible-cli search "$(printf 'a%.0s' {1..1001})"  # Too long
crucible-cli search "query\0malicious"    # Null character
```

### Query Normalization

Input queries are automatically cleaned:

- **Leading/Trailing Whitespace**: Removed automatically
- **Multiple Spaces**: Collapsed to single space
- **Tab Characters**: Converted to spaces
- **Newline Characters**: Removed from query strings

**Example**:
```bash
# This query:
crucible-cli search "   machine   learning   "

# Becomes: "machine learning"
```

## 🚨 Error Handling

### Error Categories

1. **Validation Errors**: Input doesn't meet requirements
2. **File System Errors**: Permission issues, missing files
3. **Memory Errors**: Resource limits exceeded
4. **Encoding Errors**: UTF-8 issues (handled gracefully)

### Error Message Format

All error messages follow this pattern:
```
[Error Type]: [Specific description]. [Guidance if applicable].
```

**Examples**:
```bash
# Validation Error
Error: Search query too short (1 < 2 characters). Please provide a more specific query.

# File System Error
Error: Kiln path does not exist: /invalid/path. Please set OBSIDIAN_VAULT_PATH to a valid kiln directory.

# Memory Error
Error: File too large (16MB > 10MB limit): /path/to/large-file.md

# UTF-8 Error (handled gracefully)
# Note: UTF-8 errors don't show errors, they're handled automatically
```

## 📊 Performance Considerations

### Memory Usage

- **Constant Memory**: CLI uses consistent memory regardless of vault size
- **Streaming Processing**: Large files processed without loading entire content into memory
- **Buffer Management**: 8KB buffers balance performance and memory usage
- **Garbage Collection**: Efficient memory cleanup after each file

### Performance Metrics

| Operation | Typical Performance | Memory Usage |
|-----------|-------------------|---------------|
| **Small Files** (<1MB) | <10ms per file | <1MB |
| **Medium Files** (1-10MB) | 10-100ms per file | <1MB (streaming) |
| **Large Files** (>10MB) | Skipped immediately | 0MB |
| **Query Processing** | <1ms per query | <1KB |

### Optimization Tips

1. **Use Specific Queries**: More specific queries reduce processing time
2. **Limit Results**: Use `--limit` to reduce output processing
3. **Choose Appropriate Format**: JSON format is faster than table for large result sets
4. **Organize Files**: Keep individual markdown files under 10MB for best performance

## 🔧 Troubleshooting

### Common Issues and Solutions

#### "Search query too short" Error
**Problem**: Query is only 1 character
**Solution**: Use more descriptive queries (2+ characters)
```bash
# ❌ Too short
crucible-cli search "a"

# ✅ Better
crucible-cli search "ai research"
```

#### "File too large" Warning
**Problem**: File exceeds 10MB limit
**Solution**: Split large files or exclude them from search
```bash
# Split large file
split -b 5MB large-file.md large-file-part-

# Or search in specific directory only
crucible-cli search "query" --in-dir ./notes/
```

#### UTF-8 Character Issues
**Problem**: Special characters not displaying correctly
**Solution**: CLI handles UTF-8 automatically, but ensure your terminal supports Unicode
```bash
# Check terminal UTF-8 support
echo " café résumé 🚀" | hexdump -C

# ✅ Should show proper UTF-8 encoding
```

#### Memory Issues on Large Vaults
**Problem**: System running out of memory with many files
**Solution**: Use limits and filters to reduce processing scope
```bash
# Limit number of results
crucible-cli search "query" --limit 50

# Search specific directories only
find ./notes -name "*.md" -exec crucible-cli search "query" {} +
```

### Debug Mode

Enable verbose logging for troubleshooting:
```bash
# Enable debug output
crucible-cli --verbose search "query"

# Show configuration
crucible-cli config show
```

## 🧪 Testing Safety Features

### Validation Testing

Test the safety features with these examples:

```bash
# Test query length validation
for len in 1 2 999 1000 1001; do
    query=$(printf 'a%.0s' $(seq 1 $len))
    echo "Testing length $len:"
    crucible-cli search "$query" 2>&1 | head -1
done

# Test UTF-8 handling
echo " café résumé 🚀" | crucible-cli search "café"

# Test large file handling (create test file first)
dd if=/dev/zero of=large-test.md bs=1M count=15
crucible-cli search "test"  # Should skip the large file
```

## 🔮 Future Enhancements

Planned safety improvements:

- **Configurable Limits**: User-adjustable file size and memory limits
- **Background Processing**: Non-blocking large file processing
- **Advanced UTF-8 Handling**: More sophisticated encoding error recovery
- **Performance Monitoring**: Built-in performance metrics and reporting
- **Batch Processing**: Optimized processing of multiple files

---

## 📞 Support

If you encounter issues with CLI safety features:

1. **Check Error Messages**: Read the full error message for guidance
2. **Enable Debug Mode**: Use `--verbose` for detailed information
3. **Consult Troubleshooting**: Review the troubleshooting section above
4. **Report Issues**: Include error messages and system information in bug reports

For more information, see:
- [CLI Reference](./CLI_REFERENCE.md) - Complete command documentation
- [Troubleshooting](./TROUBLESHOOTING.md) - Common issues and solutions
- [System Requirements](./SYSTEM_REQUIREMENTS.md) - Hardware and software requirements