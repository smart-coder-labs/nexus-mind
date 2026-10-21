# F5 lookback recommendations (pilot)

`GET /v1/factory/lookback` returns read-only recommendations that connect
repeated completed tasks with existing NexusMind memories whose type is
`bugfix`. An optional `project` query parameter scopes both sources.

The pilot uses a deterministic lexical rule: a term must occur in at least two
visible tasks with status `done`, and also occur in a visible, active `bugfix`
memory's title, content, topic key, or tags. For project scoped memories, task
evidence must be from the same project; memories with `scope=org` may match
across visible projects. Stop words and terms shorter than three characters are
ignored. The response includes matched terms, task IDs/titles and the memory ID
so a person can inspect why it was suggested.

The endpoint requires both `task:read` and `memory:read`. It scans up to 500
visible completed tasks and 500 visible bugfix memories, returns at most 20
recommendations, and reports whether either source was truncated. It does not
write memories, tasks, ActionPolicy, trajectories, or autonomous runs, and it
does not schedule or execute any action. This pilot uses the current task
history directly and does not depend on a trajectory store.
