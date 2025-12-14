# Grammar Design

## Output Format

Model outputs either a tool call OR prose. Never mixed.

```
output ::= tool_call | prose
```

## Tool Call Syntax

Function-call style with named parameters:

```
tool_call ::= tool_name "(" params? ")"
tool_name ::= "read" | "write" | "edit" | "ls" | "git" | "rg"
params    ::= param ("," ws param)*
param     ::= ident ws "=" ws value
```

## Value Types

```
value  ::= string | number | bool | null
string ::= "\"" chars "\""
number ::= "-"? digits ("." digits)?
bool   ::= "true" | "false"
```

## Tool Schemas (L0 + L1)

```
read(path: string, offset?: number, length?: number)
write(path: string, content: string)
edit(path: string, search: string, replace: string)
ls(path: string, depth?: number)
git(args: string)  # passthrough
rg(pattern: string, path?: string, flags?: string)
```

## GBNF Sketch

```gbnf
root ::= tool-call | prose

tool-call ::= tool-name "(" params? ")"
tool-name ::= "read" | "write" | "edit" | "ls" | "git" | "rg"

params ::= param ("," ws param)*
param ::= ident ws "=" ws value
ident ::= [a-z_]+

value ::= string | number | bool
string ::= "\"" ([^"\\] | "\\" .)* "\""
number ::= "-"? [0-9]+ ("." [0-9]+)?
bool ::= "true" | "false"

ws ::= [ \t\n]*
prose ::= [^r][^e][^a][^d].*  # anything not starting with tool name
```

Note: The prose rule needs refinement - probably use a discriminator token.

## Alternative: JSON Schema

If raw GBNF doesn't pass through llama-swap, fall back to JSON schema:

```json
{
  "type": "object",
  "properties": {
    "tool": { "enum": ["read", "write", "edit", "ls", "git", "rg"] },
    "params": { "type": "object" }
  },
  "required": ["tool", "params"]
}
```

Less elegant but universally supported.
