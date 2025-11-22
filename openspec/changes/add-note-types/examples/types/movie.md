---
type_id: movie
name: Movie
description: Movie reviews and watchlist
icon: 🎬
color: purple
relations:
  title:
    type: text
    required: true
    description: Movie title
  director:
    type: link
    target_type: person
    description: Director(s)
  rating:
    type: number
    min: 0
    max: 10
    step: 0.5
    description: Personal rating out of 10
  release_year:
    type: number
    min: 1888
    max: 2100
    description: Year of release
  watched_date:
    type: date
    description: Date you watched it
  genre:
    type: list
    options: [action, comedy, drama, horror, sci-fi, documentary, animation, thriller, romance, fantasy, mystery]
    description: Movie genre(s)
  runtime_minutes:
    type: number
    description: Runtime in minutes
  imdb_id:
    type: text
    pattern: '^tt[\d]{7,8}$'
    description: IMDb ID (e.g., tt0816692)
  imdb_rating:
    type: number
    min: 0
    max: 10
    description: IMDb rating
  cast:
    type: list
    item_type: link
    target_type: person
    description: Main cast members
  studio:
    type: text
    description: Production studio
  status:
    type: enum
    options: [watchlist, watched, rewatched, abandoned]
    default: watchlist
    description: Viewing status
  language:
    type: text
    default: English
    description: Original language
templates:
  - quick-rating
  - detailed-review
---

# Movie Type

Track movies you've watched and want to watch.

## Usage

Create a new movie note:
```bash
cru new movie "Interstellar"
```

With template:
```bash
cru new movie --template detailed-review "The Matrix"
```

## Example Queries

Find all 9+ rated sci-fi movies:
```
type:movie AND genre:sci-fi AND rating:>=9
```

Movies in watchlist:
```
type:movie AND status:watchlist
```

Movies by director:
```
type:movie AND director:[[Christopher Nolan]]
```

Movies watched this month:
```
type:movie AND watched_date:>=2025-11-01
```

Average personal rating:
```
type:movie AND status:watched --aggregate avg(rating)
```

## Relations Reference

- **title** (required): Movie title
- **director**: Link to director's person note
- **rating**: Your personal rating (0-10)
- **release_year**: Year of release
- **watched_date**: When you watched it
- **genre**: One or more genres
- **runtime_minutes**: Length in minutes
- **imdb_id**: IMDb identifier (tt format)
- **imdb_rating**: Official IMDb rating
- **cast**: Links to actor person notes
- **studio**: Production company
- **status**: Viewing status
- **language**: Original language

## Example Note

```markdown
---
type: movie
title: "Interstellar"
director: [[Christopher Nolan]]
rating: 9.5
release_year: 2014
watched_date: 2025-11-20
genre: [sci-fi, drama]
runtime_minutes: 169
imdb_id: tt0816692
imdb_rating: 8.7
cast:
  - [[Matthew McConaughey]]
  - [[Anne Hathaway]]
studio: Paramount Pictures
status: watched
language: English
tags: [movies, sci-fi, space]
---

# Interstellar

## Plot Summary

A team of explorers travel through a wormhole in space...

## My Thoughts

Visually stunning meditation on time, love, and survival...

## Favorite Scenes

- The docking scene
- Miller's planet (time dilation)
- Tesseract sequence

## Themes

- Love transcending dimensions
- Sacrifice for future generations
- Relativity and time
```
