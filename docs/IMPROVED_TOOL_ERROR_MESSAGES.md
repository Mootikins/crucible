# Improved Tool Error Messages

## Summary

This document describes the improvements made to tool argument validation error messages in the Crucible CLI REPL.

## Problem

Previously, when users called tools with incorrect arguments, they received generic count-based error messages:

```
list_files requires exactly 1 argument, got 2
create_note requires at least 3 arguments, got 1
```

These errors didn't indicate:
- Which parameters were required
- What types those parameters should be
- How to get more information about the tool

## Solution

Error messages now include:
- **Parameter names** with their types
- **Clear indication** of what's missing or wrong
- **Suggestions** to use `:help <tool>` for details
- **Optional parameters** when applicable

## Examples

### Before vs After

#### list_files

**Before:**
```
list_files requires exactly 1 argument, got 0
```

**After:**
```
Missing required parameter: path (string).
Got 0 arguments, expected 1.
Use :help list_files for details.
```

#### create_note

**Before:**
```
create_note requires at least 3 arguments: path, title, content
```

**After (no args):**
```
Missing required parameters: path (string), title (string), content (string).
Optional: tags (array of strings).
Got 0 arguments, expected at least 3.
Use :help create_note for details.
```

**After (1 arg):**
```
Missing required parameters: title (string), content (string).
Optional: tags (array of strings).
Got 1 argument, expected at least 3.
Use :help create_note for details.
```

#### search_by_tags

**Before:**
```
search_by_tags requires at least 1 argument (tag list)
```

**After:**
```
Missing required parameter: tags (array of strings).
Provide at least one tag.
Optional: match_all (boolean).
Use :help search_by_tags for details.
```

#### semantic_search

**Before:**
```
semantic_search requires exactly 1 argument, got 2
```

**After:**
```
Missing required parameter: query (string).
Optional: limit (integer).
Got 2 arguments, expected 1.
Use :help semantic_search for details.
```

## Implementation Details

### Files Modified

1. **`crates/crucible-cli/src/commands/repl/tools/system_tool_group.rs`**
   - Updated `convert_args_to_params()` method to generate descriptive error messages
   - Each tool now has custom error messages showing:
     - Required parameter names and types
     - Optional parameter names and types
     - Actual vs expected argument counts
     - Suggestion to use `:help <tool>`

2. **`crates/crucible-cli/src/commands/repl/mod.rs`**
   - Enhanced error display in `run_tool()` method
   - Strips "Parameter conversion failed:" prefix for cleaner output
   - Shows errors in red with clear formatting

### Approach

**Option A** (Implemented): Improve existing error messages
- Updated each hardcoded error message in `convert_args_to_params()`
- Added parameter names and types to error strings
- Maintained existing validation logic
- Quick to implement and test

**Option B** (Future): Schema-based validation
- Could create a helper function that validates arguments against schema
- Would generate error messages dynamically from schema
- More maintainable long-term but requires more refactoring

The current implementation uses Option A but is structured to allow migration to Option B in the future.

## Testing

Added comprehensive unit tests in `system_tool_group::tests`:

- `test_improved_error_messages_for_list_files`
- `test_improved_error_messages_for_create_note`
- `test_improved_error_messages_for_search_by_tags`
- `test_improved_error_messages_for_semantic_search`
- `test_no_arg_tools_error_messages`
- `test_successful_parameter_conversion`

All tests verify:
- Error messages contain parameter names and types
- Error messages suggest using `:help <tool>`
- Correct arguments still work as expected

## Tools Updated

The following commonly-used tools have improved error messages:

- `list_files` - requires: path (string)
- `read_file` - requires: path (string)
- `create_note` - requires: path, title, content (all strings); optional: tags (array)
- `search_by_content` - requires: query (string); optional: case_sensitive, limit
- `search_by_tags` - requires: tags (array); optional: match_all
- `semantic_search` - requires: query (string); optional: limit
- `search_documents` - requires: query (string); optional: top_k, filters
- `execute_command` - requires: command (string); optional: args (array)
- `system_info`, `get_kiln_stats`, `get_index_stats`, `get_environment` - no arguments

## Benefits

1. **Improved User Experience**: Users immediately know what went wrong and how to fix it
2. **Self-Documenting**: Error messages guide users to the help system
3. **Reduced Friction**: Less trial-and-error when learning tool usage
4. **Type Awareness**: Users learn parameter types through errors
5. **Consistency**: All tools now provide similar quality error messages

## Future Enhancements

Potential improvements for future iterations:

1. **Schema-based validation**: Dynamically generate errors from tool schemas
2. **Suggest corrections**: "Did you mean...?" for similar parameter names
3. **Example usage**: Include example invocation in error message
4. **Parameter validation**: Validate types, not just counts
5. **Multi-language support**: Translatable error messages
