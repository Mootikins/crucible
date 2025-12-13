# Test Case Format

## TOML Structure

```toml
[case]
name = "read_simple_file"
prompt = "Read the contents of README.md"

[expected]
tool = "read"
params = { path = "README.md" }

# Optional: for live mode
[setup]
create_file = { path = "README.md", content = "# Test\nHello world" }

[verify]
file_read = "README.md"
```

## Test Categories

### L0: Filesystem

```toml
# read_file.toml
[[cases]]
name = "read_simple"
prompt = "Show me what's in src/main.rs"
expected = { tool = "read", params = { path = "src/main.rs" } }

[[cases]]
name = "read_with_offset"
prompt = "Read lines 10-20 of the config file"
expected = { tool = "read", params = { path = "config", offset = 10, length = 10 } }
```

```toml
# write_file.toml
[[cases]]
name = "write_new"
prompt = "Create a file called test.txt with 'hello world'"
expected = { tool = "write", params = { path = "test.txt", content = "hello world" } }
```

```toml
# edit_file.toml
[[cases]]
name = "simple_replace"
prompt = "In main.rs, change 'foo' to 'bar'"
expected = { tool = "edit", params = { path = "main.rs", search = "foo", replace = "bar" } }
```

### L1: Tools

```toml
# git.toml
[[cases]]
name = "git_status"
prompt = "Show the git status"
expected = { tool = "git", params = { args = "status" } }

[[cases]]
name = "git_diff"
prompt = "What changed in the last commit?"
expected = { tool = "git", params = { args = "diff HEAD~1" } }
```

```toml
# rg.toml
[[cases]]
name = "search_function"
prompt = "Find all occurrences of 'async fn execute'"
expected = { tool = "rg", params = { pattern = "async fn execute" } }

[[cases]]
name = "search_in_dir"
prompt = "Search for TODO comments in the src folder"
expected = { tool = "rg", params = { pattern = "TODO", path = "src" } }
```

## Scoring Rubric

| Metric | Score | Criteria |
|--------|-------|----------|
| Parse | 0/1 | Output matches grammar |
| Tool | 0/1 | Correct tool selected |
| Params | 0-1 | Jaccard similarity of param keys + fuzzy match values |
| Task | 0/1 | (live) Operation succeeded |

## Baseline Expectations

- Unconstrained: ~70-80% parse rate, variable param accuracy
- Constrained: 100% parse rate, hopefully higher param accuracy
- Goal: Demonstrate constrained > unconstrained on smaller models
