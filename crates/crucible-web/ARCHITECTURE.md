# crucible-web Architecture Notes

## Current: Axum + Tokio Channels

Using axum for HTTP/SSE with tokio broadcast channels for internal communication.
This matches the existing workspace (crucible-acp uses axum).

```
Browser ←─SSE─→ Axum Handler ←─broadcast::Receiver─→ ChatService ←─ACP─→ Claude
                     │                                    │
                     └──────── POST /api/chat ────────────┘
```

## Future: Actix Actor Migration

Consider migrating to actix actors if/when:
- Plugin system needs proper actor supervision
- Multiple agent types need isolated state machines
- Complex event routing requires actor hierarchies
- We want stronger isolation between components

Benefits of actors:
- Supervision trees (restart failed components)
- Location transparency (distribute across processes/machines)
- Mailbox-based backpressure
- Cleaner state encapsulation

The current channel-based design can be wrapped in actors later without changing the external API.
