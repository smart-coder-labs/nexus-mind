# Autonomous Agent Scheduling Specification
## Schedule types

The system MUST support manual, daily-at-local-time, and fixed-interval schedules of at least 15 minutes, and
MUST store daily schedules with an IANA timezone.

- GIVEN an admin configures daily at 06:00 in `America/Bogota`
- WHEN the scheduler computes occurrences
- THEN the persisted UTC due time corresponds to 06:00 in that timezone rather than the server timezone

## Occurrence idempotency

The system MUST atomically create at most one run for a definition revision and scheduled UTC occurrence.

- GIVEN two scheduler instances scan the same due schedule
- WHEN both attempt to enqueue it
- THEN one run exists and the schedule advances once

## Misfires and DST

The default misfire policy MUST collapse eligible missed occurrences into one catch-up run within 24 hours.
Fall-back duplicate local times MUST run once; spring-forward nonexistent times MUST run at the next valid instant.

## Leasing and recovery

Workers MUST claim runs with expiring leases and heartbeats. An expired lease MAY produce a new attempt for the
same run, but MUST NOT change output idempotency identities or permit duplicate external writes.

## Concurrency and backpressure

The scheduler MUST enforce configured per-definition, per-repository, per-organization, and global worker
concurrency limits. Excess work remains durable and visible; it MUST NOT be dropped from an in-memory queue.
