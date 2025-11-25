# Known Bugs

## Fixed ✅

### 1. Backslash Escaping in File Paths with Spaces
**Severity:** High
**Status:** ✅ **FIXED** - 2025-11-24
**Found:** 2025-11-24

**Description:**
Files with spaces in their names caused SurrealDB parse errors and path collisions due to the `sanitize_id()` function converting spaces to underscores.

**Impact:**
- Files with spaces in names failed to process
- Path collisions: "My File.md" and "My_File.md" mapped to same record
- Merkle trees and enriched notes could not be stored
- Approximately 55+ files affected in typical vault

**Root Cause:**
The `sanitize_id()` function was replacing spaces (and other special characters) with underscores, causing information loss and collisions.

**Fix Implemented:**
Replaced `sanitize_id()` with URL encoding (`urlencoding::encode()`) in all merkle_persistence.rs methods:
- `store_tree()` - line 198
- `retrieve_tree()` - line 305
- `delete_tree()` - line 461
- `get_tree_metadata()` - line 581
- `update_tree_incremental()` - line 504

**Tests:** 16/16 new tests passing, 223/223 regression tests passing

---

### 2. No Environment Variable Support for Kiln Path
**Severity:** Medium
**Status:** ✅ **FIXED** - 2025-11-24
**Found:** 2025-11-24

**Description:**
There was no environment variable to override the kiln path. The path could only be set via config file.

**Impact:**
- Cannot override kiln path for testing without modifying config file
- Makes testing and CI/CD more difficult
- Inconsistent with other tools that support environment variables

**Fix Implemented:**
Added `CRUCIBLE_KILN_PATH` environment variable support in `config.rs` line 566:
- Priority order: CLI args > env vars > config file > defaults
- Environment variable checked after loading config file
- Properly overrides config file values

**Tests:** 3/3 new tests passing, 22/22 config tests passing

---

### 3. Enrichment Failures Without Clear Error Messages
**Severity:** Low
**Status:** ✅ **FIXED** - 2025-11-24
**Found:** 2025-11-24

**Description:**
55+ files failed with generic "Failed to store enriched note" error without detailed root cause information.

**Impact:**
- Difficult to diagnose why specific files fail
- Was likely related to bug #1 (backslash escaping) which is now resolved
- Error messages lacked file-specific context for debugging

**Root Cause:**
Error messages in the pipeline used generic `.context()` strings without file paths or phase-specific details. The original 55+ enrichment failures were likely caused by Bug #1 (path escaping).

**Fix Implemented:**
Enhanced error messages in `note_pipeline.rs` to include:
- File path in all error messages using `.with_context(|| format!())`
- Phase identification (Phase 2/4/5)
- Operation-specific context (e.g., "processing N changed blocks")
- Lines modified: 195-197, 271-275, 304-307, 311-314, 318-321, 224-232

**Examples of improved errors:**
- Phase 2: `"Phase 2: Failed to parse markdown file '/path/to/note.md'"`
- Phase 4: `"Phase 4: Failed to enrich note '/path/to/note.md' (processing 5 changed blocks)"`
- Phase 5: `"Phase 5: Failed to store enriched note for '/path/to/note.md'"`

**Tests:** All existing pipeline and storage tests passing (227/227)
