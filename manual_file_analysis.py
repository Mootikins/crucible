#!/usr/bin/env python3

import re
from collections import defaultdict, Counter

def extract_file_errors(error_log_file):
    """Extract file-specific error information"""

    with open(error_log_file, 'r') as f:
        lines = f.readlines()

    file_errors = defaultdict(list)
    current_error = []
    current_file = None

    for i, line in enumerate(lines):
        if line.startswith('error['):
            # Extract file from the next line
            if i + 1 < len(lines):
                next_line = lines[i + 1]
                file_match = re.search(r'--> ([^:]+):', next_line)
                if file_match:
                    current_file = file_match.group(1)
                    file_errors[current_file].append(line.strip())

    return dict(file_errors)

def analyze_specific_errors():
    """Analyze specific error patterns in detail"""

    with open('test_errors_raw.log', 'r') as f:
        content = f.read()

    print("# Detailed Test Compilation Error Analysis")
    print()

    # Extract all errors with file context
    error_blocks = re.findall(r'(error\[E\d+\][^\n]*\n(?:[^\n]*\n)*?--> ([^:]+):[^\n]*)', content, re.MULTILINE)

    file_error_counts = Counter()
    file_error_details = defaultdict(list)

    for error_text, file_path in error_blocks:
        file_error_counts[file_path] += 1
        file_error_details[file_path].append(error_text.split('\n')[0])

    print("## Files with Most Errors:")
    for file_path, count in file_error_counts.most_common(20):
        short_path = file_path.replace('/home/moot/crucible/', '')
        print(f"{short_path:<60} {count:3d} errors")
    print()

    print("## Error Patterns by File:")
    for file_path, count in file_error_counts.most_common(10):
        short_path = file_path.replace('/home/moot/crucible/', '')
        print(f"\n### {short_path} ({count} errors):")

        # Get dominant error types for this file
        error_codes = Counter()
        for error in file_error_details[file_path]:
            code_match = re.search(r'error\[E(\d+)\]', error)
            if code_match:
                error_codes[f"E{code_match.group(1)}"] += 1

        for code, count in error_codes.most_common(5):
            print(f"  - {code}: {count} occurrences")

    print()
    print("## Top 10 Most Problematic Test Files:")
    print()

    for file_path, count in file_error_counts.most_common(10):
        short_path = file_path.replace('/home/moot/crucible/', '')
        print(f"### {count} errors: {short_path}")

        # Show sample errors for this file
        sample_errors = file_error_details[file_path][:3]
        for error in sample_errors:
            print(f"   - {error}")
        if len(file_error_details[file_path]) > 3:
            print(f"   - ... and {len(file_error_details[file_path]) - 3} more")
        print()

if __name__ == "__main__":
    analyze_specific_errors()