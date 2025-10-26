#!/usr/bin/env python3

import re
from collections import defaultdict, Counter
import sys

def analyze_test_errors(error_log_file):
    """Comprehensive analysis of Rust test compilation errors"""

    with open(error_log_file, 'r') as f:
        content = f.read()

    # Extract all error lines
    error_lines = [line for line in content.split('\n') if line.startswith('error[')]

    # Data structures for analysis
    files_with_errors = Counter()
    error_codes = Counter()
    error_types = Counter()
    duplicate_functions = Counter()
    missing_imports = Counter()
    missing_fields = defaultdict(list)
    missing_methods = Counter()
    missing_types = Counter()

    # Detailed errors by file
    detailed_errors = defaultdict(list)

    print(f"Found {len(error_lines)} compilation errors")
    print("=" * 80)

    for line in error_lines:
        # Extract file path
        file_match = re.search(r'--> ([^:]+):', line)
        if file_match:
            file_path = file_match.group(1)
            files_with_errors[file_path] += 1
            detailed_errors[file_path].append(line)

        # Extract error code
        code_match = re.search(r'error\[E(\d+)\]', line)
        if code_match:
            error_code = code_match.group(1)
            error_codes[f"E{error_code}"] += 1

            # Categorize by error type
            error_type = get_error_type(error_code)
            error_types[error_type] += 1

        # Extract specific patterns
        extract_specific_patterns(line, duplicate_functions, missing_imports,
                                missing_fields, missing_methods, missing_types)

    return {
        'total_errors': len(error_lines),
        'files_with_errors': dict(files_with_errors),
        'error_codes': dict(error_codes),
        'error_types': dict(error_types),
        'duplicate_functions': dict(duplicate_functions),
        'missing_imports': dict(missing_imports),
        'missing_fields': dict(missing_fields),
        'missing_methods': dict(missing_methods),
        'missing_types': dict(missing_types),
        'detailed_errors': dict(detailed_errors)
    }

def get_error_type(error_code):
    """Map error codes to categories"""
    error_categories = {
        '0428': 'Duplicate Definitions',
        '0252': 'Import Conflicts',
        '0277': 'Trait Bound Issues',
        '0308': 'Type Mismatches',
        '0432': 'Unresolved Imports',
        '0433': 'Failed Resolution',
        '0116': 'Impl Definition Errors',
        '0117': 'Trait Implementation Errors',
        '0119': 'Conflict Errors',
        '0603': 'Function Definition Issues',
        '0609': 'Field Access Errors',
        '0195': 'Lifetime/Bound Mismatches',
        '0583': 'Module File Issues',
        '0061': 'Expression Parse Errors',
        '0063': 'Field Access Errors'
    }

    return error_categories.get(error_code, f'Other (E{error_code})')

def extract_specific_patterns(line, duplicate_functions, missing_imports,
                            missing_fields, missing_methods, missing_types):
    """Extract specific error patterns"""

    # Duplicate functions
    if 'the name `' in line and '` is defined multiple times' in line:
        func_match = re.search(r'the name `([^`]+)` is defined multiple times', line)
        if func_match:
            duplicate_functions[func_match.group(1)] += 1

    # Missing imports
    if 'unresolved import' in line:
        import_match = re.search(r'unresolved import `([^`]+)`', line)
        if import_match:
            missing_imports[import_match.group(1)] += 1

    # Missing types/modules
    if 'could not find' in line:
        type_match = re.search(r'could not find `([^`]+)`', line)
        if type_match:
            missing_types[type_match.group(1)] += 1

    # Missing methods
    if 'no method named' in line:
        method_match = re.search(r'no method named `([^`]+)`', line)
        if method_match:
            missing_methods[method_match.group(1)] += 1

    # Missing fields
    if 'no field `' in line and 'on type' in line:
        field_match = re.search(r'no field `([^`]+)` on type `([^`]+)`', line)
        if field_match:
            missing_fields[field_match.group(2)].append(field_match.group(1))

def generate_analysis_report(analysis):
    """Generate comprehensive analysis report"""

    print("# Comprehensive Test Compilation Error Analysis")
    print(f"Generated: {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()

    # Summary
    print("## Summary Statistics")
    print(f"- Total Files with Errors: {len(analysis['files_with_errors'])}")
    print(f"- Total Compilation Errors: {analysis['total_errors']}")
    print(f"- Duplicate Functions: {len(analysis['duplicate_functions'])}")
    print(f"- Missing Imports: {len(analysis['missing_imports'])}")
    print(f"- Missing Types/Modules: {len(analysis['missing_types'])}")
    print(f"- Missing Methods: {len(analysis['missing_methods'])}")
    print()

    # Files with most errors
    print("## Files with Most Errors (Top 15):")
    sorted_files = sorted(analysis['files_with_errors'].items(), key=lambda x: x[1], reverse=True)
    for file_path, count in sorted_files[:15]:
        short_path = file_path.replace('/home/moot/crucible/', '')
        print(f"{short_path:<70} {count:3d} errors")
    print()

    # Error code distribution
    print("## Error Code Distribution:")
    sorted_codes = sorted(analysis['error_codes'].items(), key=lambda x: x[1], reverse=True)
    for code, count in sorted_codes:
        print(f"{code:<8} {count:3d} occurrences")
    print()

    # Error type distribution
    print("## Error Type Distribution:")
    sorted_types = sorted(analysis['error_types'].items(), key=lambda x: x[1], reverse=True)
    for error_type, count in sorted_types:
        print(f"{error_type:<30} {count:3d} occurrences")
    print()

    # Critical issues
    if analysis['duplicate_functions']:
        print("## Duplicate Functions (CRITICAL ISSUE):")
        for func, count in sorted(analysis['duplicate_functions'].items(), key=lambda x: x[1], reverse=True):
            print(f"{func:<50} {count} occurrences")
        print()

    # Missing imports
    if analysis['missing_imports']:
        print("## Missing Imports:")
        for imp, count in sorted(analysis['missing_imports'].items(), key=lambda x: x[1], reverse=True)[:20]:
            print(f"{imp:<60} {count} occurrences")
        print()

    # Missing types
    if analysis['missing_types']:
        print("## Missing Types/Modules:")
        for type_name, count in sorted(analysis['missing_types'].items(), key=lambda x: x[1], reverse=True)[:20]:
            print(f"{type_name:<60} {count} occurrences")
        print()

    # Missing methods
    if analysis['missing_methods']:
        print("## Missing Methods:")
        for method, count in sorted(analysis['missing_methods'].items(), key=lambda x: x[1], reverse=True)[:20]:
            print(f"{method:<50} {count} occurrences")
        print()

def generate_prioritization_guide(analysis):
    """Generate prioritized fix guide"""

    print("# Error Fix Prioritization Guide")
    print(f"Generated: {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()

    print("## Priority 1: Critical Infrastructure Issues")
    print("These errors prevent basic compilation and should be fixed first:")
    print()

    # Files with > 10 errors
    high_error_files = [(f, c) for f, c in analysis['files_with_errors'].items() if c > 10]
    if high_error_files:
        print("### Files with > 10 errors (High Priority):")
        for file_path, count in sorted(high_error_files, key=lambda x: x[1], reverse=True):
            short_path = file_path.replace('/home/moot/crucible/', '')
            print(f"- {short_path}: {count} errors")
            # Analyze dominant error types for this file
            if file_path in analysis['detailed_errors']:
                errors = analysis['detailed_errors'][file_path]
                code_counts = Counter()
                for error in errors:
                    code_match = re.search(r'error\[E(\d+)\]', error)
                    if code_match:
                        code_counts[f"E{code_match.group(1)}"] += 1
                dominant_code = code_counts.most_common(1)[0][0] if code_counts else "Unknown"
                dominant_type = get_error_type(dominant_code[1:]) if dominant_code != "Unknown" else "Unknown"
                print(f"  → Dominant issue: {dominant_type} ({dominant_code})")
        print()

    print("## Priority 2: Systematic Issues")
    print("These indicate architectural problems affecting multiple files:")
    print()

    # Most common missing imports (systematic dependency issues)
    if analysis['missing_imports']:
        print("### Most Common Missing Imports (Dependency Issues):")
        for imp, count in sorted(analysis['missing_imports'].items(), key=lambda x: x[1], reverse=True)[:10]:
            print(f"- `{imp}`: {count} files")
        print()

    # Most common error types
    print("### Most Common Error Categories:")
    sorted_types = sorted(analysis['error_types'].items(), key=lambda x: x[1], reverse=True)
    for error_type, count in sorted_types[:10]:
        print(f"- {error_type}: {count} occurrences")
        if "Import" in error_type or "Unresolved" in error_type:
            print(f"  → *Likely fix: Check module structure and visibility*")
        elif "Duplicate" in error_type:
            print(f"  → *Likely fix: Rename or remove conflicting definitions*")
        elif "Type" in error_type:
            print(f"  → *Likely fix: Check type signatures and trait implementations*")
    print()

    print("## Priority 3: Specific Technical Debt")
    print("These are smaller issues that can be addressed after core compilation:")
    print()

    if analysis['missing_methods']:
        print("### Missing Methods:")
        for method, count in sorted(analysis['missing_methods'].items(), key=lambda x: x[1], reverse=True)[:10]:
            print(f"- `{method}`: {count} occurrences")
        print()

    print("## Recommended Fix Strategy")
    print()
    print("1. **Phase 1 - Foundation Fixes** (1-2 days)")
    print("   - Fix duplicate function names")
    print("   - Resolve module structure issues (E0583)")
    print("   - Fix basic import resolution (E0432, E0433)")
    print()
    print("2. **Phase 2 - Type System Issues** (2-3 days)")
    print("   - Fix trait implementation issues (E0116, E0117)")
    print("   - Resolve type mismatches (E0308)")
    print("   - Fix lifetime and bound issues (E0195)")
    print()
    print("3. **Phase 3 - API Compatibility** (1-2 days)")
    print("   - Fix method signature mismatches")
    print("   - Resolve field access issues (E0609)")
    print("   - Clean up unused imports and dead code")
    print()
    print("4. **Phase 4 - Validation** (1 day)")
    print("   - Run full test suite")
    print("   - Verify all compilation errors resolved")
    print("   - Update documentation if needed")

if __name__ == "__main__":
    import datetime
    from collections import Counter

    if len(sys.argv) != 2:
        print("Usage: python3 comprehensive_error_analysis.py <error_log_file>")
        sys.exit(1)

    error_log_file = sys.argv[1]
    analysis = analyze_test_errors(error_log_file)

    # Generate reports
    with open('test_compilation_analysis.md', 'w') as f:
        import sys
        original_stdout = sys.stdout
        sys.stdout = f
        try:
            generate_analysis_report(analysis)
        finally:
            sys.stdout = original_stdout

    with open('error_prioritization.md', 'w') as f:
        import sys
        original_stdout = sys.stdout
        sys.stdout = f
        try:
            generate_prioritization_guide(analysis)
        finally:
            sys.stdout = original_stdout

    print(f"Analysis complete!")
    print(f"- Detailed report: test_compilation_analysis.md")
    print(f"- Prioritization guide: error_prioritization.md")
    print(f"- Total errors analyzed: {analysis['total_errors']}")