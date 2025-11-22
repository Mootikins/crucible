---
type_id: book
name: Book
description: Book reviews and reading notes
icon: 📚
color: blue
relations:
  title:
    type: text
    required: true
    description: Book title
  author:
    type: link
    target_type: person
    description: Author (link to person note)
  rating:
    type: number
    min: 1
    max: 5
    step: 0.5
    description: Rating out of 5 stars
  genre:
    type: list
    options: [fiction, non-fiction, sci-fi, fantasy, mystery, biography, technical, history, philosophy]
    description: Book genre(s)
  read_date:
    type: date
    description: Date finished reading
  isbn:
    type: text
    pattern: '^[\d-]{10,17}$'
    description: ISBN-10 or ISBN-13
  pages:
    type: number
    description: Number of pages
  publisher:
    type: text
    description: Publishing company
  status:
    type: enum
    options: [to-read, reading, finished, abandoned]
    default: to-read
    description: Reading status
  series:
    type: text
    description: Book series name
  series_number:
    type: number
    description: Book number in series
  language:
    type: text
    default: English
    description: Original language
templates:
  - quick-review
  - detailed-analysis
---

# Book Type

This type is used for book reviews, reading notes, and book metadata tracking.

## Usage

Create a new book note with:
```bash
cru new book "The Three-Body Problem"
```

Or specify a template:
```bash
cru new book --template detailed-analysis "Project Hail Mary"
```

## Example Queries

Find all 5-star sci-fi books:
```
type:book AND genre:sci-fi AND rating:5
```

List unfinished books:
```
type:book AND status:reading
```

Books by author:
```
type:book AND author:[[Ted Chiang]]
```

Books read this year:
```
type:book AND read_date:>2025-01-01
```

Average rating across all books:
```
type:book --aggregate avg(rating)
```

## Relations Reference

- **title** (required): Book title
- **author**: Link to person note for author
- **rating**: 1-5 stars (0.5 increments allowed)
- **genre**: One or more genres from predefined list
- **read_date**: Date you finished reading
- **isbn**: ISBN-10 or ISBN-13 (with or without dashes)
- **pages**: Page count
- **publisher**: Publishing company
- **status**: Current reading status
- **series**: Name of book series (if applicable)
- **series_number**: Position in series
- **language**: Original publication language

## Example Note

```markdown
---
type: book
title: "The Three-Body Problem"
author: [[Liu Cixin]]
rating: 5
genre: [sci-fi, hard-sci-fi]
read_date: 2025-11-15
isbn: 978-0765382030
pages: 400
publisher: Tor Books
status: finished
series: "Remembrance of Earth's Past"
series_number: 1
language: Chinese
tags: [books, reading, sci-fi, chinese-literature]
---

# The Three-Body Problem

## Summary

First contact story set against the backdrop of China's Cultural Revolution...

## Key Themes
- Civilizational communication challenges
- Game theory and survival
- Scientific determinism vs chaos

## Favorite Quotes

> "In the shooter hypothesis, a good marksman shoots..."

## My Thoughts

Absolutely brilliant hard sci-fi...
```
