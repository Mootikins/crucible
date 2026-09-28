## User

edit the library

### Tool: `edit_file` (id: c1)

```json
{
  "path": "src/lib.rs",
  "old_string": "a",
  "new_string": "b"
}
```

#### Result (id: c1)

```
ok
```

## Assistant

Done.

## User

fix main

### Tool: `Edit` (id: c2)

```json
{
  "file_path": "src/main.rs"
}
```

#### Result (id: c2)

```
edited
```

> **Error:** The turn failed: agent turn error: LLM timeout

## User

try again

> **Error:** The turn failed: agent turn error: stopped
