---
type_id: meeting
name: Meeting
description: Meeting notes and action items
icon: 🗓️
color: green
relations:
  title:
    type: text
    required: true
    description: Meeting title/topic
  meeting_date:
    type: date
    required: true
    default: today
    description: Date of meeting
  attendees:
    type: list
    item_type: link
    target_type: person
    description: People who attended
  facilitator:
    type: link
    target_type: person
    description: Person running the meeting
  meeting_type:
    type: enum
    options: [standup, planning, review, retrospective, sync, brainstorm, one-on-one, all-hands]
    description: Type of meeting
  duration_minutes:
    type: number
    description: Meeting length in minutes
  location:
    type: text
    description: Physical or virtual location
  agenda_items:
    type: list
    item_type: text
    description: Planned agenda topics
  action_items:
    type: list
    item_type: text
    description: Action items and owners
  decisions:
    type: list
    item_type: text
    description: Key decisions made
  next_meeting:
    type: date
    description: Next scheduled meeting
  recording_url:
    type: text
    description: Link to meeting recording
  status:
    type: enum
    options: [scheduled, completed, cancelled]
    default: scheduled
templates:
  - standup
  - planning-meeting
  - retrospective
  - one-on-one
---

# Meeting Type

Structured meeting notes with action items tracking.

## Usage

Create new meeting notes:
```bash
cru new meeting "Sprint Planning"
```

With template:
```bash
cru new meeting --template standup "Daily Standup"
```

## Example Queries

All meetings this week:
```
type:meeting AND meeting_date:>=2025-11-18
```

Retrospective meetings:
```
type:meeting AND meeting_type:retrospective
```

Meetings with specific person:
```
type:meeting AND attendees:[[John Doe]]
```

Upcoming scheduled meetings:
```
type:meeting AND status:scheduled AND meeting_date:>{{date}}
```

## Relations Reference

- **title** (required): Meeting topic
- **meeting_date** (required): When the meeting occurred/will occur
- **attendees**: List of participants
- **facilitator**: Meeting lead
- **meeting_type**: Category of meeting
- **duration_minutes**: How long it lasted
- **location**: Where it took place
- **agenda_items**: Topics to cover
- **action_items**: Tasks and owners
- **decisions**: Key outcomes
- **next_meeting**: Follow-up date
- **recording_url**: Link to recording
- **status**: Scheduling status

## Example Note

```markdown
---
type: meeting
title: "Sprint Planning - Q4 Features"
meeting_date: 2025-11-22
attendees:
  - [[Alice Johnson]]
  - [[Bob Smith]]
  - [[Carol Davis]]
facilitator: [[Alice Johnson]]
meeting_type: planning
duration_minutes: 90
location: Conference Room A
status: completed
tags: [meetings, sprint-planning, q4]
---

# Sprint Planning - Q4 Features

Date: November 22, 2025
Duration: 90 minutes

## Attendees
- Alice Johnson (facilitator)
- Bob Smith
- Carol Davis

## Agenda
1. Review Q4 roadmap
2. Break down user stories
3. Estimate story points
4. Commit to sprint goals

## Discussion Notes

### Q4 Roadmap Review
Main priorities discussed...

### Story Breakdown
- Feature A: 8 points
- Feature B: 5 points
- Feature C: 13 points (needs further breakdown)

## Decisions
- [ ] Commit to Features A and B for this sprint
- [ ] Split Feature C into smaller stories
- [ ] Schedule design review for next week

## Action Items
- [ ] @Bob: Create detailed specs for Feature A (Due: Nov 24)
- [ ] @Carol: Design mockups for Feature B (Due: Nov 25)
- [ ] @Alice: Break down Feature C (Due: Nov 23)

## Next Steps
Next sprint planning: December 6, 2025
```
