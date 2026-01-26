#!/usr/bin/env python3
"""
Example of AI-calling tools from Python code.
This shows how tool schemas provide enough type information.
"""

# The actual tool definitions with inferred types from schemas
read_file = lambda path: read_file(path=path)  # path: str
write_file = lambda path, content: write_file(path=path, content=content)  # path: str, content: str
bash = lambda command, background=False: bash(command=command, background=background)  # command: str, background: bool
grep = lambda pattern, path=None, glob=None: grep(pattern=pattern, path=path, glob=glob)  # pattern: str
glob = lambda pattern, limit=100: glob(pattern=pattern, limit=limit)  # pattern: str, limit: int

def analyze_python_file(file_path: str) -> None:
    """
    Example workflow that calls multiple tools with inferred types.
    The types are inferred from the function schemas provided.
    """
    print(f"Analyzing {file_path}...")
    
    # Read file - path is str, returns str content
    content = read_file(path=file_path)
    
    # Find Python files in directory - pattern is str, returns list of paths
    files = glob(pattern='**/*.py', limit=50)
    
    # Search for specific patterns - pattern is str
    imports = grep(pattern=r'^import |^from ', path=file_path)
    functions = grep(pattern=r'^def ', path=file_path)
    
    # Execute commands - command is str
    bash(command='python -m py_compile ' + file_path)
    
    # Conditional tool use
    if len(functions) > 10:
        print(f"Found {len(functions)} functions in {file_path}")
    
    # Background task example - boolean parameter
    bash(command='python -m pytest', background=True)

def build_project() -> None:
    """Build a project using multiple tools"""
    print("Starting build...")
    
    # Setup
    bash(command='npm install', timeout_ms=120000)
    
    # Build
    result = bash(command='npm run build')
    
    # Test (background)
    bash(command='npm test', background=True)
    
    # Deploy
    if result == 0:  # Success check inferred from bash result
        bash(command='npm run deploy')

def search_codebase(query: str) -> None:
    """Search codebase for patterns"""
    print(f"Searching for: {query}")
    
    # Multiple search strategies
    results = grep(pattern=query, limit=100)
    
    files_with_content = []
    for result in results:
        # Each grep result is a line with content
        path, line = result.rsplit(':', 1)
        content = read_file(path=path)
        lines = content.split('\n')
        line_num = int(line)
        
        if line_num > 0 and line_num <= len(lines):
            files_with_content.append({
                'path': path,
                'line': line_num,
                'content': lines[line_num - 1].strip()
            })
    
    return files_with_content

if __name__ == '__main__':
    # Example usage
    analyze_python_file('example.py')
    
    results = search_codebase('TODO')
    print(f"Found {len(results)} TODOs")
