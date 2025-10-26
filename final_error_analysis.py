#!/usr/bin/env python3

import re
from collections import defaultdict, Counter

def comprehensive_error_analysis():
    """Create comprehensive manual analysis of test compilation errors"""

    with open('test_errors_raw.log', 'r') as f:
        content = f.read()

    print("# Comprehensive Test Compilation Error Analysis")
    print(f"Generated: 2025-10-25 22:06:00")
    print()

    # Extract all error blocks with file information
    error_pattern = r'error\[E(\d+)\]([^\n]*\n)(?:[^\n]*\n)*?\s*--> ([^:]+):(\d+):(\d+)'

    matches = re.findall(error_pattern, content)

    print(f"## Summary Statistics")
    print(f"- Total Compilation Errors: {len(matches)}")
    print()

    # Analyze by error codes
    error_codes = Counter()
    error_types = Counter()
    file_errors = defaultdict(list)

    for code, desc, file, line, col in matches:
        error_codes[f"E{code}"] += 1
        file_errors[file].append((f"E{code}", desc.strip()))

        # Categorize error types
        category = get_error_category(code)
        error_types[category] += 1

    print("## Error Code Distribution (Top 15):")
    for code, count in error_codes.most_common(15):
        print(f"{code:<8} {count:3d} occurrences")
    print()

    print("## Error Type Distribution:")
    for category, count in error_types.most_common():
        print(f"{category:<30} {count:3d} occurrences")
    print()

    print("## Files with Most Errors (Top 15):")
    file_counts = {file: len(errors) for file, errors in file_errors.items()}
    for file, count in sorted(file_counts.items(), key=lambda x: x[1], reverse=True)[:15]:
        short_path = file.replace('/home/moot/crucible/', '')
        print(f"{short_path:<70} {count:3d} errors")
    print()

    # Detailed analysis of most problematic files
    print("## Detailed Analysis of High-Error Files:")
    print()

    for file, count in sorted(file_counts.items(), key=lambda x: x[1], reverse=True)[:10]:
        if count >= 5:  # Only show files with 5+ errors
            short_path = file.replace('/home/moot/crucible/', '')
            print(f"### {short_path} ({count} errors):")

            # Show dominant error types for this file
            file_error_codes = Counter()
            for code, desc in file_errors[file]:
                file_error_codes[code] += 1

            print("  **Dominant Error Types:**")
            for code, err_count in file_error_codes.most_common(5):
                category = get_error_category(code[1:])
                print(f"  - {code} ({category}): {err_count} occurrences")

            # Show sample error messages
            print("  **Sample Errors:**")
            for code, desc in file_errors[file][:3]:
                print(f"  - {code}: {desc}")

            if len(file_errors[file]) > 3:
                print(f"  - ... and {len(file_errors[file]) - 3} more")
            print()

    # Specific pattern analysis
    print("## Specific Error Pattern Analysis:")
    print()

    # Missing imports pattern
    import_errors = []
    for file, errors in file_errors.items():
        for code, desc in errors:
            if "unresolved import" in desc:
                import_match = re.search(r'unresolved import `([^`]+)`', desc)
                if import_match:
                    import_errors.append((import_match.group(1), file))

    if import_errors:
        print("### Missing Imports (Systematic Issue):")
        import_counts = Counter()
        for imp, file in import_errors:
            import_counts[imp] += 1

        for imp, count in import_counts.most_common(10):
            print(f"- `{imp}`: {count} files")
        print()

    # Method/field access issues
    method_errors = []
    field_errors = []

    for file, errors in file_errors.items():
        for code, desc in errors:
            if "no method named" in desc:
                method_match = re.search(r'no method named `([^`]+)`', desc)
                if method_match:
                    method_errors.append((method_match.group(1), file, code))
            elif "no field" in desc and "on type" in desc:
                field_match = re.search(r'no field `([^`]+)` on type `([^`]+)`', desc)
                if field_match:
                    field_errors.append((field_match.group(1), field_match.group(2), file, code))

    if method_errors:
        print("### Missing Methods:")
        method_counts = Counter()
        for method, file, code in method_errors:
            method_counts[method] += 1

        for method, count in method_counts.most_common(10):
            print(f"- `{method}`: {count} occurrences")
        print()

    # Async/await issues
    async_errors = []
    for file, errors in file_errors.items():
        for code, desc in errors:
            if "is not a future" in desc or "cannot be applied to values that implement `Try`" in desc:
                async_errors.append((desc, file, code))

    if async_errors:
        print("### Async/Await Issues:")
        print(f"- Total async-related errors: {len(async_errors)}")
        print("  These typically indicate missing `.await` or incorrect Result handling")
        print()

    return len(matches)

def get_error_category(code):
    """Map error codes to human-readable categories"""

    categories = {
        '0432': 'Unresolved Import',
        '0433': 'Failed Resolution',
        '0277': 'Trait Bound Issue',
        '0308': 'Type Mismatch',
        '0599': 'Method Not Found',
        '0608': 'Indexing Error',
        '0609': 'Field Access Error',
        '0382': 'Move/Borrow Error',
        '0116': 'Impl Definition Error',
        '0117': 'Trait Implementation Error',
        '0195': 'Lifetime/Bound Mismatch',
        '0583': 'Module File Issue',
        '0061': 'Expression Parse Error',
        '0425': 'Closure/Borrow Error',
        '0716': 'Temporary Value Error',
        '0616': 'Private Field Access',
        '0600': 'Unary Operator Error',
        '0046': 'Method Not Found',
        '0368': 'Borrow Checker Error',
        '0369': 'Borrow Checker Error'
    }

    return categories.get(code, f'Other (E{code})')

def create_prioritization_guide(total_errors):
    """Create detailed prioritization guide"""

    print("# Test Compilation Error Fix Prioritization Guide")
    print()
    print(f"**Total Errors to Fix: {total_errors}**")
    print()

    print("## 🚨 Priority 1: Critical Infrastructure (Fix First)")
    print("These errors block basic compilation and affect multiple files:")
    print()

    print("### 1. Module Structure & Import Issues")
    print("- **E0432/E0433: Unresolved imports** (24 total)")
    print("  - Most common: `crate::common`, `crate::test_utilities`")
    print("  - Fix strategy: Check module visibility and crate structure")
    print("  - Files affected: Multiple test files")
    print()

    print("### 2. Async/Await Core Issues")
    print("- **E0277: Not a future / Try trait issues** (51 total)")
    print("  - Fix strategy: Add `.await`, check Result handling")
    print("  - Pattern: Many async functions missing `.await`")
    print()

    print("### 3. Type System Foundation")
    print("- **E0308: Type mismatches** (41 total)")
    print("  - Fix strategy: Check function signatures and return types")
    print()

    print("## ⚡ Priority 2: API Compatibility (Quick Wins)")
    print("These are relatively easy fixes that unlock many tests:")
    print()

    print("### 1. Method Access Issues")
    print("- **E0599: Method not found** (41 total)")
    print("  - Common missing methods: `len`, `is_empty`, `get_tools`, `contains_key`")
    print("  - Fix strategy: Check if methods exist or need to be imported")
    print()

    print("### 2. Field Access & Privacy")
    print("- **E0609/E0616: Field access errors** (13 total)")
    print("  - Fix strategy: Make fields public or provide accessor methods")
    print()

    print("### 3. Indexing & Collection Issues")
    print("- **E0608: Cannot index into Result** (18 total)")
    print("  - Fix strategy: Handle Result before indexing")
    print()

    print("## 🔧 Priority 3: Code Quality & Cleanup")
    print("These should be fixed after core compilation works:")
    print()

    print("- **E0382: Move/borrow errors** (11 total)")
    print("- **E0716/E0425: Borrow checker issues** (6 total)")
    print("- Various lifetime and closure issues")
    print()

    print("## 📋 Recommended Fix Strategy")
    print()

    print("### Phase 1: Foundation (Days 1-2)")
    print("1. **Fix module imports** - `crate::common`, `crate::test_utilities`")
    print("2. **Add missing .await calls** - Look for \"not a future\" errors")
    print("3. **Fix Result handling** - Check \"Try trait\" errors")
    print("4. **Resolve type mismatches** - Function signatures and returns")
    print()

    print("### Phase 2: API Compatibility (Days 3-4)")
    print("1. **Fix method calls** - Check if methods exist or need imports")
    print("2. **Handle Result types** - Unwrap results before indexing")
    print("3. **Fix field access** - Make fields public or add accessors")
    print()

    print("### Phase 3: Code Quality (Day 5)")
    print("1. **Fix borrow checker issues**")
    print("2. **Clean up move errors**")
    print("3. **Handle remaining edge cases**")
    print()

    print("### Phase 4: Validation (Day 6)")
    print("1. **Run full test suite**")
    print("2. **Verify all errors resolved**")
    print("3. **Update documentation**")
    print()

    print("## 🎯 Success Metrics")
    print()
    print("- **Phase 1 Goal**: Reduce errors from 221 to ~100")
    print("- **Phase 2 Goal**: Reduce errors from ~100 to ~30")
    print("- **Phase 3 Goal**: Reduce errors from ~30 to 0")
    print("- **Total Estimated Time**: 5-6 days")
    print()

if __name__ == "__main__":
    total_errors = comprehensive_error_analysis()
    create_prioritization_guide(total_errors)