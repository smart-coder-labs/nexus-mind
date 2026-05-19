# NexusMind Enterprise Demo — Step-by-Step Guide

**Estimated duration**: 8–10 minutes  
**Audience**: CTO, VP Engineering, Compliance Officer  
**Login key**: `nm_demo_acme_admin` (Acme Corp — admin)

---

## Setup (before the call)

```bash
# Option A — Docker (recommended)
docker compose up -d
./scripts/reset-demo.sh

# Option B — local dev
cd apps/backend && cargo run &
./scripts/reset-demo.sh
cd apps/admin && npm run dev
```

Verify at `http://localhost:8080/v1/health` → `{"status":"ok"}`  
Admin panel: `http://localhost:3000`

---

## Scene 1: "Up and running in 2 commands" (1 min)

> *"Your team can have this running in 2 commands — no cloud, no vendor lock-in."*

- Show terminal: `docker compose up -d` + health check output
- Open `http://localhost:3000`, enter key `nm_demo_acme_admin`
- Dashboard loads → **Acme Corp**

**Talking point**: self-hosted, SQLite, zero external dependencies.

---

## Scene 2: Dashboard (1 min)

> *"Here's Acme Corp. 5 active developers, 20 memories stored, searches happening today."*

- Point to the 4 stat cards
- Scroll the activity feed: different users, different tools, all captured

**Talking point**: real-time visibility into how your team uses AI.

---

## Scene 3: User Management (2 min)

> *"Every developer has their own API key. You control who has access."*

- **Users page** — show the 5 team members: Sarah Chen (admin), Marcus Johnson, Ana García, David Park
- Click **Invite user** → fill in name, email, role → copy the key shown once
- Click **Revoke** on one user → *"They're out immediately. Key stops working."*
- Click **Rotate key** → *"Need to cycle credentials? Done in one click."*

**Talking point**: enterprise-grade access control, no shared secrets.

---

## Scene 4: Memory Browser (2 min)

> *"Every memory your team's AI tools have saved — searchable, auditable, yours."*

- **Memories page** — table with 20 entries from 3 different tools
- Search `"authentication"` → results appear (debounced, no button needed)
- Click a row → full content, tags, timestamp, who stored it
- Filter: clear search, open a memory tagged `["payments", "stripe"]`
- Click **Export CSV** → *"Compliance team wants a report? One click."*

**Talking point**: full-text search across all team knowledge.

---

## Scene 5: Audit Trail (1 min)

> *"You know exactly what happened, who did it, and when."*

- **Audit Log page** — entries with timestamps, users, actions color-coded
- Filter by action `store` → only storage events
- Filter by user `Sarah Chen` → her activity only
- *"Ayer 14:32, Ana García stored 'switch to OAuth2'."*
- **Export CSV** → hand it to your security team

**Talking point**: SOC 2 / compliance-ready audit trail out of the box.

---

## Scene 6: Settings (30 sec)

- **Settings page** — org name, API key rotation
- *"Everything configurable, nothing locked down."*

---

## Scene 7: Close (1 min)

> *"Questions? You can have this running in your own infrastructure this week."*

**Key differentiators to emphasize**:
1. **Self-hosted** — your data never leaves your servers
2. **Multi-user** — team-wide memory with individual keys
3. **Audit trail** — complete compliance story
4. **2-command setup** — `docker compose up` + `reset-demo.sh`

**Next steps**:
- Send them the GitHub repo link
- Offer a 30-min technical deep-dive with their engineering team
- Trial: run it in their staging environment this week

---

## Demo Keys (seed data)

| Organization | Role  | Key                     |
|-------------|-------|-------------------------|
| Acme Corp   | admin | `nm_demo_acme_admin`    |
| Acme Corp   | member| `nm_demo_acme_sarah`    |
| TechStartup | admin | `nm_demo_techstartup_admin` |
| DevShop     | admin | `nm_demo_devshop_admin` |

> Keys are deterministic — running `reset-demo.sh` always produces the same keys.
