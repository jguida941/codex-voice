# ADR 0008: Transcript Queue Overflow Handling

Status: Accepted
Date: 2026-01-29

## Context

When transcripts arrive faster than they can be injected (e.g., prompt not ready,
user speaking rapidly), they accumulate in a queue. Without limits:
- Memory grows unboundedly
- Old transcripts become stale and confusing
- System can become unresponsive

Need a strategy for handling overflow.

## Decision

Implement bounded FIFO queue with drop-oldest policy:
- Hard limit: 5 pending transcripts (`MAX_PENDING_TRANSCRIPTS`)
- When full: Drop oldest transcript, keep newest
- Notify user: Status line shows "Queue full - oldest dropped"
- Merge optimization: Same-mode transcripts are merged before sending

## Consequences

**Positive:**
- Bounded memory usage (max 5 transcripts in queue)
- Most recent speech prioritized (likely more relevant)
- User is notified when drops occur
- Merging reduces injection count

**Negative:**
- Old speech can be lost silently (except for notification)
- User may not notice the notification
- Limit of 5 is arbitrary (but seems reasonable in practice)

**Trade-offs:**
- Chose recent-speech priority over FIFO fairness
- Small queue (5) keeps memory tight but may drop in burst scenarios

## Alternatives Considered

- **Unbounded queue**: Memory risk; stale transcripts accumulate.
- **Drop newest**: Loses most recent (likely most relevant) speech.
- **Block on full**: Would stall voice capture; bad UX.
- **Larger queue**: Delays problem but doesn't solve it.

## Links

- `src/src/bin/voiceterm/transcript/queue.rs` - Queue limit constant + drop logic
- `src/src/bin/voiceterm/transcript/delivery.rs` - Merge logic
