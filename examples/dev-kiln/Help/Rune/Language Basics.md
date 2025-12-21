---
title: Rune Language Basics
description: Fundamentals of the Rune scripting language for Crucible plugins
status: implemented
tags:
  - rune
  - scripting
  - reference
---

# Rune Language Basics

Rune is the scripting language used for Crucible plugins. It's designed to be familiar to Rust developers while being lightweight and embeddable.

## Variables

```rune
// Immutable binding (default)
let name = "value";

// Mutable binding
let mut count = 0;
count += 1;
```

## Types

```rune
let text = "string";           // String
let number = 42;               // Integer
let decimal = 3.14;            // Float
let flag = true;               // Boolean
let items = ["a", "b", "c"];   // Array
let map = #{ key: "value" };   // Object (hash map)
```

## Functions

```rune
/// Public function (callable from outside)
pub fn greet(name) {
    format!("Hello, {}!", name)
}

// Private function (internal only)
fn helper() {
    // internal use
}
```

**Note:** The last expression is the return value (no `return` needed).

## Control Flow

### Conditionals

```rune
if condition {
    // ...
} else if other {
    // ...
} else {
    // ...
}
```

### Loops

```rune
// For loop
for item in items {
    println(item);
}

// While loop
while condition {
    // ...
}

// Loop with break
loop {
    if done {
        break;
    }
}
```

### Match

```rune
match value {
    Some(x) => println("Got: {}", x),
    None => println("Nothing"),
}
```

## Error Handling

Rune uses `Result` like Rust:

```rune
// Return a Result
pub fn might_fail() {
    if something_wrong {
        return Err("error message");
    }
    Ok(result)
}

// Propagate errors with ?
let value = might_fail()?;

// Handle errors explicitly
match might_fail() {
    Ok(v) => println("Got: {}", v),
    Err(e) => println("Error: {}", e),
}
```

## Option Type

```rune
// Check for None
if let Some(value) = optional {
    // Use value
}

// Provide default
let value = optional.unwrap_or(default);
```

## String Operations

```rune
let s = "hello world";

s.contains("world")      // true
s.starts_with("hello")   // true
s.split(" ")             // ["hello", "world"]
s.replace("world", "you") // "hello you"
format!("{} {}", a, b)   // String formatting
```

## Collections

### Arrays

```rune
let arr = [1, 2, 3];

arr.len()           // 3
arr.push(4)         // [1, 2, 3, 4]
arr.get(0)          // Some(1)
arr.iter()          // Iterator

for item in arr {
    println(item);
}
```

### Hash Maps (Objects)

```rune
let map = #{
    name: "Alice",
    age: 30
};

map.get("name")           // Some("Alice")
map.contains("age")       // true
map.insert("city", "NYC") // Add/update
```

## Common Patterns

### Option Handling

```rune
// Safe access with default
let name = user.get("name").unwrap_or("Anonymous");

// Conditional processing
if let Some(email) = user.get("email") {
    send_notification(email);
}
```

### Error Recovery

```rune
pub fn safe_operation() {
    match risky_call() {
        Ok(result) => {
            // Process result
            Ok(result)
        }
        Err(e) => {
            println("Warning: {}", e);
            Ok(default_value)  // Recover with default
        }
    }
}
```

### Iteration with Index

```rune
for (i, item) in items.iter().enumerate() {
    println("[{}] {}", i, item);
}
```

## Printing and Debugging

```rune
println("Simple message");
println("Value: {}", value);
println("Debug: {:?}", complex_value);
```

## See Also

- [[Help/Rune/Crucible API]] - Available functions
- [[Help/Rune/Best Practices]] - Writing good plugins
- [[Help/Extending/Creating Plugins]] - Plugin overview
