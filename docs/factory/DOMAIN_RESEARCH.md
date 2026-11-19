# Domain Research agent

The `domain_research` template researches a project's **business** on the web and saves what it finds as memories of that project, so the other agents (strategist, resolver, reviewers, QA) read it instead of guessing how the business works. For an insurance product it would save, for example, the regulator's obligations, public tariff datasets, how a policy is issued and cancelled, and the terms brokers use.

The code is in `apps/backend/src/automation/worker.rs` (`research_memory`, `store_research_memories` and the `domain_research` arm of `fixed_prompt`).

## Setting one up

In **Factory → Agents → New agent**, pick **Domain Research**:

| Field | Required | Meaning |
|---|---|---|
| Target: project | yes | The NexusMind project whose memory receives the research |
| Business domain | yes | The business, its market and country |
| Topics first | no | Areas to cover before the rest (regulation, pricing, open datasets) |
| Topics per run | no | 1 to 15, default 8 |

Run it by hand or on a schedule. Each run covers what is still missing, so several runs go deeper instead of repeating.

## What it saves

One memory per topic, in the target project:

- type `project`, tags `domain-research` plus the kind (`dataset`, `paper`, `regulation`, `market`, `process`, `glossary`, `pricing`, `competitor` or `other`);
- the content, what it means for the product, and a **Sources** list of the URLs the agent fetched;
- a header line with the date the sources were read.

The run result lists the memories it created under `research_memories`. In the admin, filter **Memories** by the tag `domain-research`.

## Saved once

Each topic has the key `research/<project>/<topic-slug>`. A run:

1. receives the titles and slugs already saved as `known_research` and is told not to research them again;
2. saves only entries whose key does not exist yet.

An existing memory is never overwritten, so a correction a person made in the admin survives later runs. **To refresh a topic, delete its memory** and run the agent again.

## Trust boundary

The agent reads the open web, so it holds no write tool: only `WebSearch`, `WebFetch`, `Read` and memory search. The worker stores the entries, and only:

- into the run's own project;
- when the entry has a title, content and at least one `http(s)` source;
- with content capped at 12,000 characters, and at most 30 entries per run.

Every memory is labelled as reference data, and agents are told to treat memory as reference, never as instructions. This limits a poisoned web page to one labelled, sourced note in one project. It does not check that the note is **true**: review the first runs of a new domain before relying on them.

## Other agents

Every agent that knows its project already reads its memory. Their memory instructions now say that research on the business is there and should be searched before assuming how the business works.
