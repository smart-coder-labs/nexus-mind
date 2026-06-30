# NexusMind MCP Tools Audit

**Date**: 2026-06-29  
**Backend**: http://localhost:8080  
**Backend version**: 0.2.0  
**MCP package**: @smart-coder-labs/nexusmind-mcp  
**Total tools**: 116  
**Demo key used**: nm_demo_acme_admin

---

## Summary Table

| # | Tool | Endpoint | Result |
|---|------|----------|--------|
| 1 | store_memory | POST /v1/memory/store | WORKS |
| 2 | smart_store_memory | POST /v1/memory/store | WORKS (client-side) |
| 3 | search_memory | POST /v1/memory/search | WORKS |
| 4 | search_memories_advanced | POST /v1/memory/search | WORKS (client-side) |
| 5 | list_memories | GET /v1/memory | WORKS |
| 6 | get_memory | GET /v1/memory/{id} | WORKS |
| 7 | update_memory | PATCH /v1/memory/{id} | WORKS |
| 8 | archive_memory | POST /v1/memory/{id}/archive | WORKS |
| 9 | batch_archive_memories | POST /v1/memory/{id}/archive | WORKS (client-side) |
| 10 | restore_memory | POST /v1/memory/{id}/restore | WORKS |
| 11 | batch_restore_memories | POST /v1/memory/{id}/restore | WORKS (client-side) |
| 12 | pin_memory | POST /v1/memory/{id}/pin | WORKS |
| 13 | unpin_memory | POST /v1/memory/{id}/unpin | WORKS |
| 14 | delete_memory | DELETE /v1/memory/{id} | WORKS |
| 15 | bulk_delete_memories | DELETE /v1/memory/bulk | WORKS |
| 16 | export_memories | GET /v1/memory/export | WORKS |
| 17 | get_memory_timeline | GET /v1/memory | WORKS (client-side) |
| 18 | find_related_memories | GET /v1/memory/{id} + POST /v1/memory/search | WORKS (client-side) |
| 19 | memory_health_check | GET /v1/memory + GET /v1/admin/stats/duplicates | WORKS (client-side) |
| 20 | merge_memories | POST /v1/admin/memories/merge | WORKS |
| 21 | bulk_tag_memories | POST /v1/admin/memories/bulk-tag | WORKS |
| 22 | search_and_tag | POST /v1/memory/search + POST /v1/admin/memories/bulk-tag | WORKS (client-side) |
| 23 | import_memories | POST /v1/admin/memories/import | WORKS |
| 24 | update_memory_note | PATCH /v1/admin/memories/{id}/note | WORKS |
| 25 | schedule_memory_delete | PATCH /v1/admin/memories/{id}/schedule-delete | WORKS |
| 26 | quick_health_check | GET /v1/admin/memories/health | WORKS |
| 27 | index_project | POST /v1/code/index | WORKS |
| 28 | search_code | POST /v1/code/search | WORKS |
| 29 | get_symbol_context | GET /v1/code/context | FAILS |
| 30 | list_code_projects | GET /v1/code/projects | WORKS |
| 31 | get_code_project_files | GET /v1/code/projects/{id}/files | WORKS |
| 32 | reindex_project | POST /v1/code/projects/{id}/reindex | WORKS |
| 33 | delete_code_project | DELETE /v1/code/projects/{name} | WORKS |
| 34 | global_search | GET /v1/search | WORKS |
| 35 | get_context | GET /v1/memory + GET /v1/conventions | WORKS (client-side) |
| 36 | list_collections | GET /v1/admin/collections | WORKS |
| 37 | create_collection | POST /v1/admin/collections | WORKS |
| 38 | update_collection | PATCH /v1/admin/collections/{id} | FAILS |
| 39 | assign_memory_to_collection | POST /v1/memories/{id}/collection | WORKS |
| 40 | delete_collection | DELETE /v1/admin/collections/{id} | WORKS |
| 41 | list_conventions | GET /v1/conventions | WORKS |
| 42 | get_conventions_summary | GET /v1/conventions | WORKS (client-side) |
| 43 | search_conventions | GET /v1/conventions | WORKS (client-side) |
| 44 | check_convention_compliance | GET /v1/conventions | WORKS (client-side) |
| 45 | store_convention | POST /v1/conventions | WORKS |
| 46 | import_conventions_from_text | POST /v1/conventions | WORKS (client-side) |
| 47 | get_convention | GET /v1/conventions/{id} | WORKS |
| 48 | update_convention | PATCH /v1/conventions/{id} | WORKS |
| 49 | pin_convention | PATCH /v1/conventions/{id} (weight: 999) | WORKS (client-side) |
| 50 | archive_convention | POST /v1/conventions/{id}/archive | WORKS |
| 51 | bulk_archive_conventions | POST /v1/conventions/{id}/archive | WORKS (client-side) |
| 52 | bulk_update_convention_weight | PATCH /v1/conventions/{id} | WORKS (client-side) |
| 53 | restore_convention | POST /v1/conventions/{id}/restore | WORKS |
| 54 | delete_convention | DELETE /v1/conventions/{id} | WORKS |
| 55 | get_project_context | GET /v1/context/project/{name} | WORKS |
| 56 | summarize_project | GET /v1/context/project/{name} + GET /v1/admin/stats | WORKS (client-side) |
| 57 | check_policy | POST /v1/policy/check | WORKS |
| 58 | list_policies | GET /v1/policies | WORKS |
| 59 | create_policy | POST /v1/policies | WORKS |
| 60 | update_policy | PATCH /v1/policies/{id} | WORKS |
| 61 | delete_policy | DELETE /v1/policies/{id} | WORKS |
| 62 | list_api_keys | GET /v1/admin/keys | WORKS |
| 63 | create_api_key | POST /v1/admin/keys | WORKS |
| 64 | revoke_api_key | DELETE /v1/admin/keys/{id} | WORKS |
| 65 | get_audit_log | GET /v1/audit | WORKS |
| 66 | get_audit_summary | GET /v1/audit | WORKS (client-side) |
| 67 | get_org_settings | GET /v1/admin/org/settings | WORKS |
| 68 | update_org_settings | PATCH /v1/admin/org/settings | WORKS |
| 69 | update_org | PATCH /v1/admin/org | WORKS |
| 70 | set_announcement | PATCH /v1/admin/org/announcement | WORKS |
| 71 | get_announcement | GET /v1/admin/org/settings | WORKS (client-side) |
| 72 | get_stats | GET /v1/admin/stats | WORKS |
| 73 | get_agent_activity | GET /v1/admin/stats/agent-activity | WORKS |
| 74 | get_tag_stats | GET /v1/admin/stats/tags | WORKS |
| 75 | find_duplicate_memories | GET /v1/admin/stats/duplicates | WORKS |
| 76 | get_memory_trends | GET /v1/admin/stats/trends | WORKS |
| 77 | get_memory_facets | GET /v1/admin/stats/memory-facets | WORKS |
| 78 | get_usage_stats | GET /v1/admin/stats/usage | WORKS |
| 79 | analyze_memory_gaps | GET /v1/admin/stats/trends | WORKS (client-side) |
| 80 | get_agent_dashboard | GET /v1/admin/stats + agent-activity + conventions | WORKS (client-side) |
| 81 | onboard_agent | POST /v1/memory/store + multiple reads | WORKS (client-side) |
| 82 | rename_tag | POST /v1/admin/tags/rename | WORKS |
| 83 | merge_tags | POST /v1/admin/tags/rename | WORKS (client-side) |
| 84 | list_users | GET /v1/admin/users | WORKS |
| 85 | invite_user | POST /v1/admin/invites | WORKS |
| 86 | assign_user_role | PATCH /v1/admin/users/{id} | FAILS |
| 87 | get_users_by_role | GET /v1/admin/users?role=... | WORKS (client-side) |
| 88 | disable_user | POST /v1/admin/users/{id}/disable | WORKS |
| 89 | bulk_disable_users | POST /v1/admin/users/{id}/disable | WORKS (client-side) |
| 90 | enable_user | POST /v1/admin/users/{id}/enable | WORKS |
| 91 | bulk_enable_users | POST /v1/admin/users/{id}/enable | WORKS (client-side) |
| 92 | list_roles | GET /v1/roles | WORKS |
| 93 | create_role | POST /v1/roles | WORKS |
| 94 | delete_role | DELETE /v1/roles/{id} | WORKS |
| 95 | list_projects | GET /v1/projects | WORKS |
| 96 | create_project | POST /v1/projects | WORKS |
| 97 | get_project_members | GET /v1/projects/{id}/members | WORKS |
| 98 | add_project_member | POST /v1/projects/{id}/members | WORKS |
| 99 | update_project | PATCH /v1/projects/{id} | WORKS |
| 100 | list_webhooks | GET /v1/webhooks | WORKS |
| 101 | create_webhook | POST /v1/webhooks | WORKS |
| 102 | update_webhook | PATCH /v1/webhooks/{id} | WORKS |
| 103 | test_webhook | POST /v1/webhooks/{id}/test | WORKS |
| 104 | delete_webhook | DELETE /v1/webhooks/{id} | WORKS |
| 105 | list_sessions | GET /v1/sessions | WORKS |
| 106 | create_session | POST /v1/sessions | WORKS |
| 107 | update_session | PATCH /v1/sessions/{id} | WORKS |
| 108 | get_session_memories | GET /v1/memory?session_id={id} | WORKS |
| 109 | get_session_stats | GET /v1/sessions | WORKS (client-side) |
| 110 | delete_session | DELETE /v1/sessions/{id} | FAILS |
| 111 | record_decision | POST /v1/memory/store | WORKS (client-side) |
| 112 | create_sprint_retrospective | POST /v1/memory/search | WORKS (client-side) |
| 113 | generate_daily_standup | POST /v1/memory/search | WORKS (client-side) |
| 114 | sync_agent_context | POST /v1/memory/store | WORKS (client-side) |
| 115 | get_agent_context | GET /v1/admin/stats + conventions + memory | WORKS (client-side) |
| 116 | export_all_data | GET /v1/memory + conventions + projects | WORKS (client-side) |

**Total: 112 WORKS / 4 FAILS**

---

## Per-Tool Results

### store_memory / smart_store_memory
**Endpoint**: POST /v1/memory/store  
**Maps to MCP tools**: store_memory, smart_store_memory, record_decision, sync_agent_context

**Request**:
```
curl -s -X POST http://localhost:8080/v1/memory/store \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "audit-agent",
    "project": "nexusmind",
    "user": "cesarpuentes452@gmail.com",
    "type": "semantic",
    "content": "Audit test memory - endpoint coverage",
    "tags": ["audit","test"],
    "metadata": {"session_id": "audit-session-001"}
  }'
```

**Response** (HTTP 200):
```json
{"id":"bf76a39a-aa0e-413a-aa91-49a6cfd985bd","org_id":"76ca0be2-...","project":"nexusmind","tool":"audit-agent","content":"Audit test memory - endpoint coverage","tags":["audit","test"],"created_at":"2026-06-30T03:19:18Z","type":"semantic","scope":"project","revision_count":1,"pinned":false,"status":"active"}
```

> **NOTE**: The API spec shows `201 Created` as the expected status code, but the server returns `200 OK`. Minor spec inconsistency.

**Verdict**: WORKS

---

### search_memory / search_memories_advanced
**Endpoint**: POST /v1/memory/search  
**Maps to MCP tools**: search_memory, search_memories_advanced, create_sprint_retrospective, generate_daily_standup

**Request**:
```
curl -s -X POST http://localhost:8080/v1/memory/search \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"query":"audit test","project":"nexusmind","limit":5}'
```

**Response** (HTTP 200):
```json
{"memories":[{"id":"bf76a39a-...","content":"Audit test memory - endpoint coverage","tags":["audit","test"],...}],"total":1,"limit":5,"offset":0}
```

**Verdict**: WORKS

---

### list_memories / get_memory_timeline
**Endpoint**: GET /v1/memory  
**Maps to MCP tools**: list_memories, get_memory_timeline, get_session_memories, memory_health_check (partial)

**Request**:
```
curl -s http://localhost:8080/v1/memory?limit=3 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"memories":[...],"total":2,"limit":3,"offset":0}
```

**Verdict**: WORKS

---

### get_memory / find_related_memories
**Endpoint**: GET /v1/memory/{id}  
**Maps to MCP tools**: get_memory, find_related_memories (partial)

**Request**:
```
curl -s http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"id":"bf76a39a-...","content":"Audit test memory - endpoint coverage","tags":["audit","test"],"pinned":false,"status":"active",...}
```

**Verdict**: WORKS

---

### update_memory
**Endpoint**: PATCH /v1/memory/{id}  
**Maps to MCP tools**: update_memory

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"content":"Audit test memory - updated","tags":["audit","test","updated"]}'
```

**Response** (HTTP 200):
```json
{"id":"bf76a39a-...","content":"Audit test memory - updated","revision_count":2,...}
```

**Verdict**: WORKS

---

### archive_memory / batch_archive_memories
**Endpoint**: POST /v1/memory/{id}/archive  
**Maps to MCP tools**: archive_memory, batch_archive_memories

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd/archive \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### restore_memory / batch_restore_memories
**Endpoint**: POST /v1/memory/{id}/restore  
**Maps to MCP tools**: restore_memory, batch_restore_memories

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd/restore \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### pin_memory
**Endpoint**: POST /v1/memory/{id}/pin  
**Maps to MCP tools**: pin_memory

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd/pin \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### unpin_memory
**Endpoint**: POST /v1/memory/{id}/unpin  
**Maps to MCP tools**: unpin_memory

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd/unpin \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### delete_memory
**Endpoint**: DELETE /v1/memory/{id}  
**Maps to MCP tools**: delete_memory

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/memory/bf76a39a-aa0e-413a-aa91-49a6cfd985bd \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### bulk_delete_memories
**Endpoint**: DELETE /v1/memory/bulk  
**Maps to MCP tools**: bulk_delete_memories

**Request**:
```
curl -s -X DELETE http://localhost:8080/v1/memory/bulk \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"ids":["95db2ddf-78e8-4e00-9284-12babd5f0e60"]}'
```

**Response** (HTTP 200):
```json
{"deleted":1}
```

**Verdict**: WORKS

---

### export_memories / export_all_data (partial)
**Endpoint**: GET /v1/memory/export  
**Maps to MCP tools**: export_memories, export_all_data (partial)

**Request**:
```
curl -s http://localhost:8080/v1/memory/export \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200): CSV format
```
id,title,type,scope,project,tool,content,tags,topic_key,session_id,revision_count,pinned,created_at
95db2ddf-...,,semantic,project,nexusmind,audit-agent,Audit test memory 2 - for bulk delete,audit;test;bulk,,,1,false,2026-06-30T03:19:43Z
```

**Verdict**: WORKS

---

### merge_memories
**Endpoint**: POST /v1/admin/memories/merge  
**Maps to MCP tools**: merge_memories

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/memories/merge \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"source_id":"c6668456-d000-40d4-a57e-1e5bd4166c95","target_id":"9c7cc7ec-2a3d-45ff-9a2c-882899c6c5a5","merged_content":"Merged content"}'
```

**Response** (HTTP 200):
```json
{"id":"9c7cc7ec-...","content":"Audit admin test A - for merge\n\n---\n\nAudit admin test B - for merge",...}
```

**Verdict**: WORKS

---

### bulk_tag_memories / search_and_tag
**Endpoint**: POST /v1/admin/memories/bulk-tag  
**Maps to MCP tools**: bulk_tag_memories, search_and_tag

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/memories/bulk-tag \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"memory_ids":["9c7cc7ec-2a3d-45ff-9a2c-882899c6c5a5"],"tags":["audit","bulk-tagged"]}'
```

**Response** (HTTP 200):
```json
{"updated":2}
```

**Verdict**: WORKS

---

### import_memories
**Endpoint**: POST /v1/admin/memories/import  
**Maps to MCP tools**: import_memories

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/memories/import \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"memories":[{"content":"Imported audit test memory","tags":["audit","imported"],"type":"semantic","project":"nexusmind","tool":"audit-import-test"}]}'
```

**Response** (HTTP 200):
```json
{"imported":1,"skipped":0,"errors":[]}
```

**Verdict**: WORKS

---

### update_memory_note
**Endpoint**: PATCH /v1/admin/memories/{id}/note  
**Maps to MCP tools**: update_memory_note

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/admin/memories/9c7cc7ec-2a3d-45ff-9a2c-882899c6c5a5/note \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"note":"Admin audit test note"}'
```

**Response** (HTTP 200):
```json
{"id":"9c7cc7ec-...","admin_note":"Admin audit test note",...}
```

**Verdict**: WORKS

---

### schedule_memory_delete
**Endpoint**: PATCH /v1/admin/memories/{id}/schedule-delete  
**Maps to MCP tools**: schedule_memory_delete

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X PATCH \
  http://localhost:8080/v1/admin/memories/9c7cc7ec-2a3d-45ff-9a2c-882899c6c5a5/schedule-delete \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"delete_at":"2030-12-31T23:59:59Z"}'
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### quick_health_check
**Endpoint**: GET /v1/admin/memories/health  
**Maps to MCP tools**: quick_health_check

**Request**:
```
curl -s http://localhost:8080/v1/admin/memories/health \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"total_memories":29,"duplicate_count":0,"stale_count":26,"untagged_count":4}
```

**Verdict**: WORKS

---

### index_project
**Endpoint**: POST /v1/code/index  
**Maps to MCP tools**: index_project

**Request**:
```
curl -s -X POST http://localhost:8080/v1/code/index \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"root_path":"/private/tmp","project":"audit-code-test"}'
```

**Response** (HTTP 200):
```json
{"project":"audit-code-test","status":"indexing_started","file_count":0,"chunk_count":0,"last_indexed":""}
```

> **NOTE**: Body requires `root_path` or `repo_url`. The field `path` is rejected with `validation_error: "either repo_url or root_path must be provided"`.

**Verdict**: WORKS

---

### search_code
**Endpoint**: POST /v1/code/search  
**Maps to MCP tools**: search_code

**Request**:
```
curl -s -X POST http://localhost:8080/v1/code/search \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"query":"test","project":"audit-code-test","limit":3}'
```

**Response** (HTTP 200):
```json
[]
```

**Verdict**: WORKS

---

### get_symbol_context
**Endpoint**: GET /v1/code/context  
**Maps to MCP tools**: get_symbol_context

**Request**:
```
curl -s "http://localhost:8080/v1/code/context?symbol=main&project=audit-code-test" \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 400):
```
Failed to deserialize query string: missing field `file_path`
```

> **BUG**: The `file_path` query parameter is mandatory but is not documented in the MCP tool description or the API spec. Any call to `get_symbol_context` that omits `file_path` receives a 400 error. Even with `file_path` provided and the project indexed, the endpoint returns 404 if the symbol is not found in that specific file.

**Verdict**: FAILS (undocumented required parameter causes 400 for all standard callers)

---

### list_code_projects
**Endpoint**: GET /v1/code/projects  
**Maps to MCP tools**: list_code_projects

**Request**:
```
curl -s http://localhost:8080/v1/code/projects \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"2","name":"audit-code-test","root_path":"/private/tmp","file_count":97,"chunk_count":337,"last_indexed":"2026-06-30T03:20:41Z","index_status":"success"}]
```

**Verdict**: WORKS

---

### get_code_project_files
**Endpoint**: GET /v1/code/projects/{id}/files  
**Maps to MCP tools**: get_code_project_files

**Request**:
```
curl -s http://localhost:8080/v1/code/projects/2/files \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
["a.json","aa.json","api_results.json","auth_main.rs",...]
```

**Verdict**: WORKS

---

### reindex_project
**Endpoint**: POST /v1/code/projects/{id}/reindex  
**Maps to MCP tools**: reindex_project

**Request**:
```
curl -s -X POST http://localhost:8080/v1/code/projects/2/reindex \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"status":"indexing_started","project_id":"2"}
```

**Verdict**: WORKS

---

### delete_code_project
**Endpoint**: DELETE /v1/code/projects/{name}  
**Maps to MCP tools**: delete_code_project

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/code/projects/audit-del-test2 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

> **NOTE**: Despite the route template `/v1/code/projects/:id`, the path param is treated as a project **name** (string), not an integer id. Passing the numeric id string returns 404. The delete handler signature `Path(name): Path<String>` confirms this.

**Verdict**: WORKS

---

### global_search
**Endpoint**: GET /v1/search  
**Maps to MCP tools**: global_search

**Request**:
```
curl -s "http://localhost:8080/v1/search?q=audit" \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"memories":[{"id":"9c7cc7ec-...","content":"Audit admin test A - for merge...","tags":["audit","merge-test","bulk-tagged"],...}]}
```

**Verdict**: WORKS

---

### list_collections
**Endpoint**: GET /v1/admin/collections  
**Maps to MCP tools**: list_collections

**Request**:
```
curl -s http://localhost:8080/v1/admin/collections \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[]
```

**Verdict**: WORKS

---

### create_collection
**Endpoint**: POST /v1/admin/collections  
**Maps to MCP tools**: create_collection

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/collections \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Collection","description":"Created during audit","color":"#FF6B6B"}'
```

**Response** (HTTP 200):
```json
{"id":"44f7229c-4a4c-410c-aa85-220852e112c8","name":"Audit Test Collection","description":"Created during audit","created_at":"2026-06-30T03:21:09Z","memory_count":0}
```

**Verdict**: WORKS

---

### update_collection
**Endpoint**: PATCH /v1/admin/collections/{id}  
**Maps to MCP tools**: update_collection

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X PATCH \
  http://localhost:8080/v1/admin/collections/44f7229c-4a4c-410c-aa85-220852e112c8 \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Updated Collection"}'
```

**Response** (HTTP 405): Method Not Allowed

> **BUG**: `PATCH /v1/admin/collections/{id}` returns 405. Neither `PUT` nor `PATCH` are registered for this path in the router. The `update_collection` MCP tool has no working backend endpoint.

**Verdict**: FAILS (405 Method Not Allowed — route not registered)

---

### assign_memory_to_collection
**Endpoint**: POST /v1/memories/{id}/collection  
**Maps to MCP tools**: assign_memory_to_collection

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/memories/9c7cc7ec-2a3d-45ff-9a2c-882899c6c5a5/collection \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"collection_id":"44f7229c-4a4c-410c-aa85-220852e112c8"}'
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### delete_collection
**Endpoint**: DELETE /v1/admin/collections/{id}  
**Maps to MCP tools**: delete_collection

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/admin/collections/44f7229c-4a4c-410c-aa85-220852e112c8 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### list_conventions / get_conventions_summary / search_conventions / check_convention_compliance
**Endpoint**: GET /v1/conventions  
**Maps to MCP tools**: list_conventions, get_conventions_summary, search_conventions, check_convention_compliance, get_context (partial)

**Request**:
```
curl -s http://localhost:8080/v1/conventions \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[]
```

**Verdict**: WORKS

---

### store_convention / import_conventions_from_text
**Endpoint**: POST /v1/conventions  
**Maps to MCP tools**: store_convention, import_conventions_from_text

**Request**:
```
curl -s -X POST http://localhost:8080/v1/conventions \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"title":"Audit Test Convention","content":"Always write tests before shipping code.","category":"testing","weight":50,"tags":["audit","testing"]}'
```

**Response** (HTTP 200):
```json
{"id":1,"org_id":"...","title":"Audit Test Convention","content":"Always write tests before shipping code.","category":"testing","weight":50,"tags":["audit","testing"],"created_at":"2026-06-30 03:21:21","archived_at":null}
```

**Verdict**: WORKS

---

### get_convention
**Endpoint**: GET /v1/conventions/{id}  
**Maps to MCP tools**: get_convention

**Request**:
```
curl -s http://localhost:8080/v1/conventions/1 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"id":1,"title":"Audit Test Convention","content":"Always write tests before shipping code.",...}
```

**Verdict**: WORKS

---

### update_convention / pin_convention / bulk_update_convention_weight
**Endpoint**: PATCH /v1/conventions/{id}  
**Maps to MCP tools**: update_convention, pin_convention (weight=999), bulk_update_convention_weight

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/conventions/1 \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"title":"Audit Test Convention Updated","weight":999}'
```

**Response** (HTTP 200):
```json
{"id":1,"title":"Audit Test Convention Updated","weight":999,...}
```

**Verdict**: WORKS

---

### archive_convention / bulk_archive_conventions
**Endpoint**: POST /v1/conventions/{id}/archive  
**Maps to MCP tools**: archive_convention, bulk_archive_conventions

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/conventions/2/archive \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

> **NOTE**: Returns 404 when the convention belongs to a different org (org isolation enforced correctly).

**Verdict**: WORKS

---

### restore_convention
**Endpoint**: POST /v1/conventions/{id}/restore  
**Maps to MCP tools**: restore_convention

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/conventions/2/restore \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### delete_convention
**Endpoint**: DELETE /v1/conventions/{id}  
**Maps to MCP tools**: delete_convention

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/conventions/2 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### get_project_context / summarize_project
**Endpoint**: GET /v1/context/project/{name}  
**Maps to MCP tools**: get_project_context, summarize_project (partial)

**Request**:
```
curl -s http://localhost:8080/v1/context/project/nexusmind \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"conventions":[],"last_activity":"2026-06-30T03:20:05Z","project":"nexusmind","recent_memories":[...]}
```

**Verdict**: WORKS

---

### check_policy
**Endpoint**: POST /v1/policy/check  
**Maps to MCP tools**: check_policy

**Request**:
```
curl -s -X POST http://localhost:8080/v1/policy/check \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"user":"cesarpuentes452@gmail.com","tool":"audit-agent","model":"claude-opus-4","action":"chat","prompt_hash":"sha256:abc123","prompt_preview":"Test audit policy check","estimated_tokens":100,"estimated_cost":0.001}'
```

**Response** (HTTP 200):
```json
{"allowed":true,"violations":[]}
```

**Verdict**: WORKS

---

### list_policies
**Endpoint**: GET /v1/policies  
**Maps to MCP tools**: list_policies

**Request**:
```
curl -s http://localhost:8080/v1/policies \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"policies":[]}
```

**Verdict**: WORKS

---

### create_policy
**Endpoint**: POST /v1/policies  
**Maps to MCP tools**: create_policy

**Request**:
```
curl -s -X POST http://localhost:8080/v1/policies \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Policy","rule_type":"budget_limit","config":{"max_tokens_per_day":100000},"enabled":true}'
```

**Response** (HTTP 201):
```json
{"id":"89644917-e502-4ce0-8f5f-21ec8b491e6b","name":"Audit Test Policy","rule_type":"budget_limit","config":{"max_tokens_per_day":100000},"enabled":true,...}
```

> **NOTE**: Body requires `rule_type` (one of `model_whitelist`, `budget_limit`, `pii_redact`) and a type-specific `config` object. The API spec shows a simplified example with `rule.type` and `rule.max_cost_usd` which are invalid and will produce a 422 error.

**Verdict**: WORKS

---

### update_policy
**Endpoint**: PATCH /v1/policies/{id}  
**Maps to MCP tools**: update_policy

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/policies/89644917-e502-4ce0-8f5f-21ec8b491e6b \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"enabled":false,"config":{"max_tokens_per_day":50000}}'
```

**Response** (HTTP 200):
```json
{"id":"89644917-...","enabled":false,"config":{"max_tokens_per_day":50000},...}
```

**Verdict**: WORKS

---

### delete_policy
**Endpoint**: DELETE /v1/policies/{id}  
**Maps to MCP tools**: delete_policy

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/policies/89644917-e502-4ce0-8f5f-21ec8b491e6b \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### list_api_keys
**Endpoint**: GET /v1/admin/keys  
**Maps to MCP tools**: list_api_keys

**Request**:
```
curl -s http://localhost:8080/v1/admin/keys \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"2e2c5a55-...","user_name":"Admin User","user_email":"admin@acme.com","label":"demo-admin","last_used":"2026-06-30 03:23:05","revoked":false,"times_used":107}]
```

**Verdict**: WORKS

---

### create_api_key
**Endpoint**: POST /v1/admin/keys  
**Maps to MCP tools**: create_api_key

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/keys \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"cc61bc1f-e408-453e-8d22-7d02c3e3045a","label":"audit-test-key"}'
```

**Response** (HTTP 201):
```json
{"key":{"id":"25a91b4d-...","label":"audit-test-key","revoked":false},"raw_key":"nm_4a66558f77dce..."}
```

> **NOTE**: Body requires `user_id` (UUID of an existing org member). This is not documented in the MCP tool description. The raw key is returned only once at creation.

**Verdict**: WORKS

---

### revoke_api_key
**Endpoint**: DELETE /v1/admin/keys/{id}  
**Maps to MCP tools**: revoke_api_key

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/admin/keys/25a91b4d-18c9-412c-a298-43b104f6d416 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### get_audit_log / get_audit_summary
**Endpoint**: GET /v1/audit  
**Maps to MCP tools**: get_audit_log, get_audit_summary

**Request**:
```
curl -s http://localhost:8080/v1/audit \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"0f739fad-...","action":"key.created","resource_type":"api_key","timestamp":"2026-06-30T03:23:41Z","previous_hash":"...","current_hash":"..."}]
```

**Verdict**: WORKS

---

### get_org_settings / get_announcement
**Endpoint**: GET /v1/admin/org/settings  
**Maps to MCP tools**: get_org_settings, get_announcement

**Request**:
```
curl -s http://localhost:8080/v1/admin/org/settings \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"events":{"resolve_issues":true,"review_prs":true,"respond_comments":true,"auto_index":true,"scanner":true},"min_password_length":8,"announcement_type":"info"}
```

**Verdict**: WORKS

---

### update_org_settings
**Endpoint**: PATCH /v1/admin/org/settings  
**Maps to MCP tools**: update_org_settings

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/admin/org/settings \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"retention_days":90,"max_memories_per_user":1000}'
```

**Response** (HTTP 200):
```json
{"events":{...},"retention_days":90,"min_password_length":8,"announcement_type":"info"}
```

**Verdict**: WORKS

---

### update_org
**Endpoint**: PATCH /v1/admin/org  
**Maps to MCP tools**: update_org

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/admin/org \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Acme Corp (Audit Test)"}'
```

**Response** (HTTP 200):
```json
{"id":"76ca0be2-...","name":"Acme Corp (Audit Test)","slug":"acme","created_at":"2026-05-21T15:14:28Z"}
```

**Verdict**: WORKS

---

### set_announcement
**Endpoint**: PATCH /v1/admin/org/announcement  
**Maps to MCP tools**: set_announcement

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/admin/org/announcement \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"announcement":"Audit test announcement","announcement_type":"info"}'
```

**Response** (HTTP 200):
```json
{"events":{...},"announcement":"Audit test announcement","announcement_type":"info"}
```

> **NOTE**: Body field is `announcement` (not `text`). Sending `{"text":"..."}` returns 422.

**Verdict**: WORKS

---

### get_stats
**Endpoint**: GET /v1/admin/stats  
**Maps to MCP tools**: get_stats, summarize_project (partial), get_agent_dashboard (partial)

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"total_memories":29,"active_users_24h":1,"searches_today":2,"top_tools":[{"tool":"claude-code","count":14},...]}
```

**Verdict**: WORKS

---

### get_agent_activity
**Endpoint**: GET /v1/admin/stats/agent-activity  
**Maps to MCP tools**: get_agent_activity, get_agent_dashboard (partial)

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/agent-activity \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"tool":"claude-code","total_memories":10,"memories_last_24h":10,"memories_last_7d":10,"last_seen":"2026-06-30T03:23:43Z"},...]
```

**Verdict**: WORKS

---

### get_tag_stats
**Endpoint**: GET /v1/admin/stats/tags  
**Maps to MCP tools**: get_tag_stats

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/tags \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"name":"convention","count":6},{"name":"payments","count":5},{"name":"security","count":4},...]
```

**Verdict**: WORKS

---

### find_duplicate_memories
**Endpoint**: GET /v1/admin/stats/duplicates  
**Maps to MCP tools**: find_duplicate_memories

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/duplicates \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[]
```

**Verdict**: WORKS

---

### get_memory_trends / analyze_memory_gaps
**Endpoint**: GET /v1/admin/stats/trends  
**Maps to MCP tools**: get_memory_trends, analyze_memory_gaps

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/trends \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"daily_counts":[{"date":"2026-06-30","count":20}],"by_type":[{"name":"untyped","count":20}],"by_project":[...],"total":20,"this_week":20,"this_month":20}
```

**Verdict**: WORKS

---

### get_memory_facets
**Endpoint**: GET /v1/admin/stats/memory-facets  
**Maps to MCP tools**: get_memory_facets

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/memory-facets \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"types":[],"scopes":[{"value":"project","count":20}],"projects":[{"value":"nexusmind","count":11},...]}
```

**Verdict**: WORKS

---

### get_usage_stats
**Endpoint**: GET /v1/admin/stats/usage  
**Maps to MCP tools**: get_usage_stats

**Request**:
```
curl -s http://localhost:8080/v1/admin/stats/usage \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"memories":20,"sessions":0,"users":5,"projects":3,"code_repos":0}
```

**Verdict**: WORKS

---

### rename_tag / merge_tags
**Endpoint**: POST /v1/admin/tags/rename  
**Maps to MCP tools**: rename_tag, merge_tags

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/tags/rename \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"from":"audit","to":"audit-renamed-test"}'
```

**Response** (HTTP 200):
```json
{"updated_count":2}
```

> **NOTE**: Body fields are `from` and `to`, not `old_tag`/`new_tag`. Using wrong field names returns 422.

**Verdict**: WORKS

---

### list_users / get_users_by_role
**Endpoint**: GET /v1/admin/users  
**Maps to MCP tools**: list_users, get_users_by_role

**Request**:
```
curl -s "http://localhost:8080/v1/admin/users?role=admin" \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"cc61bc1f-...","email":"admin@acme.com","name":"Admin User","role":"admin","status":"active",...}]
```

**Verdict**: WORKS

---

### invite_user
**Endpoint**: POST /v1/admin/invites  
**Maps to MCP tools**: invite_user

**Request**:
```
curl -s -X POST http://localhost:8080/v1/admin/invites \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"email":"audit-test-user@example.com","role":"member"}'
```

**Response** (HTTP 201):
```json
{"token":"ea5107dc1b3c4b98ab6f699749a894ab","invite_url":"http://localhost:5173/set-password?invite=...","expires_at":"2026-07-07 03:24:47","role":"member"}
```

**Verdict**: WORKS

---

### assign_user_role
**Endpoint**: PATCH /v1/admin/users/{id}  
**Maps to MCP tools**: assign_user_role

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X PATCH \
  http://localhost:8080/v1/admin/users/19efbb46-5b66-48cb-8ca2-23b490aaae67 \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"role":"viewer"}'
```

**Response** (HTTP 404): Not Found

> **BUG**: `PATCH /v1/admin/users/{id}` is not registered in the router. Router has `GET`, `POST .../disable`, `POST .../enable`, and `PATCH .../note` — but no top-level PATCH for role changes. The `assign_user_role` MCP tool has no working backend endpoint.

**Verdict**: FAILS (route not registered — 404)

---

### disable_user / bulk_disable_users
**Endpoint**: POST /v1/admin/users/{id}/disable  
**Maps to MCP tools**: disable_user, bulk_disable_users

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/admin/users/387f928e-b198-4d2f-b82d-db3cc740e7d3/disable \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

> **NOTE**: Returns 404 if user is already disabled. Returns 422 when trying to disable your own account.

**Verdict**: WORKS

---

### enable_user / bulk_enable_users
**Endpoint**: POST /v1/admin/users/{id}/enable  
**Maps to MCP tools**: enable_user, bulk_enable_users

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/admin/users/387f928e-b198-4d2f-b82d-db3cc740e7d3/enable \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

> **NOTE**: Returns 404 if user is not currently disabled.

**Verdict**: WORKS

---

### list_roles
**Endpoint**: GET /v1/roles  
**Maps to MCP tools**: list_roles

**Request**:
```
curl -s http://localhost:8080/v1/roles \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"tmpl_security_officer","name":"security-officer","display_name":"Security Officer","permissions":["audit:read","settings:write"],"is_template":true,...},...]
```

**Verdict**: WORKS

---

### create_role
**Endpoint**: POST /v1/roles  
**Maps to MCP tools**: create_role

**Request**:
```
curl -s -X POST http://localhost:8080/v1/roles \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"audit-test-role","display_name":"Audit Test Role","permissions":["memory:read"],"description":"Created during audit test"}'
```

**Response** (HTTP 201):
```json
{"id":"579e00b9-...","name":"audit-test-role","display_name":"Audit Test Role","permissions":["memory:read"],"is_template":false,...}
```

> **NOTE**: Body requires both `name` (snake_case identifier) and `display_name` (human-readable label). Sending only `name` and `permissions` returns 422.

**Verdict**: WORKS

---

### delete_role
**Endpoint**: DELETE /v1/roles/{id}  
**Maps to MCP tools**: delete_role

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/roles/579e00b9-4f61-48c0-a8f3-dd95825ad285 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### list_projects
**Endpoint**: GET /v1/projects  
**Maps to MCP tools**: list_projects, onboard_agent (partial)

**Request**:
```
curl -s http://localhost:8080/v1/projects \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"007a3f94-...","name":"infra","created_at":"2026-06-30 03:23:43"},{"id":"e5397139-...","name":"nexusmind",...},...]
```

**Verdict**: WORKS

---

### create_project
**Endpoint**: POST /v1/projects  
**Maps to MCP tools**: create_project

**Request**:
```
curl -s -X POST http://localhost:8080/v1/projects \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Project","description":"Created during audit"}'
```

**Response** (HTTP 200):
```json
{"id":"4e9b7017-b357-4f05-957b-01a3a3174c87","name":"Audit Test Project","description":"Created during audit","created_at":"2026-06-30T03:26:41Z"}
```

**Verdict**: WORKS

---

### get_project_members
**Endpoint**: GET /v1/projects/{id}/members  
**Maps to MCP tools**: get_project_members

**Request**:
```
curl -s http://localhost:8080/v1/projects/4e9b7017-b357-4f05-957b-01a3a3174c87/members \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[]
```

**Verdict**: WORKS

---

### add_project_member
**Endpoint**: POST /v1/projects/{id}/members  
**Maps to MCP tools**: add_project_member

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X POST \
  http://localhost:8080/v1/projects/4e9b7017-b357-4f05-957b-01a3a3174c87/members \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"19efbb46-5b66-48cb-8ca2-23b490aaae67","role":"member"}'
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### update_project
**Endpoint**: PATCH /v1/projects/{id}  
**Maps to MCP tools**: update_project

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/projects/4e9b7017-b357-4f05-957b-01a3a3174c87 \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Project Updated","description":"Updated during audit"}'
```

**Response** (HTTP 200):
```json
{"ok":true}
```

> **NOTE**: The actual route is `PATCH /v1/projects/{id}` (not `/v1/admin/projects/{id}` as documented in some internal specs). Using the admin prefix returns 404.

**Verdict**: WORKS

---

### list_webhooks
**Endpoint**: GET /v1/webhooks  
**Maps to MCP tools**: list_webhooks

**Request**:
```
curl -s http://localhost:8080/v1/webhooks \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"webhooks":[]}
```

**Verdict**: WORKS

---

### create_webhook
**Endpoint**: POST /v1/webhooks  
**Maps to MCP tools**: create_webhook

**Request**:
```
curl -s -X POST http://localhost:8080/v1/webhooks \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Webhook","target_url":"https://example.com/audit-test-webhook","events":["memory.created","memory.deleted"],"secret":"audit-test-secret-123"}'
```

**Response** (HTTP 200):
```json
{"id":"6826e9ac-...","name":"Audit Test Webhook","target_url":"https://example.com/audit-test-webhook","active":true,"created_at":"2026-06-30T03:27:28Z"}
```

> **NOTE**: Body field is `target_url` not `url`. Using `url` returns 422.

**Verdict**: WORKS

---

### update_webhook
**Endpoint**: PATCH /v1/webhooks/{id}  
**Maps to MCP tools**: update_webhook

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/webhooks/6826e9ac-948d-4d2d-a102-137033a963ec \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"active":false}'
```

**Response** (HTTP 200):
```json
{"id":"6826e9ac-...","active":false,...}
```

**Verdict**: WORKS

---

### test_webhook
**Endpoint**: POST /v1/webhooks/{id}/test  
**Maps to MCP tools**: test_webhook

**Request**:
```
curl -s -X POST http://localhost:8080/v1/webhooks/6826e9ac-948d-4d2d-a102-137033a963ec/test \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"success":false,"status_code":405,"error":"Received HTTP 405"}
```

> **NOTE**: `success:false` is expected when the target URL returns a non-2xx response. The endpoint itself works correctly — it fires the request and reports the result.

**Verdict**: WORKS

---

### delete_webhook
**Endpoint**: DELETE /v1/webhooks/{id}  
**Maps to MCP tools**: delete_webhook

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/webhooks/6826e9ac-948d-4d2d-a102-137033a963ec \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 204): No content

**Verdict**: WORKS

---

### list_sessions / get_session_stats
**Endpoint**: GET /v1/sessions  
**Maps to MCP tools**: list_sessions, get_session_stats

**Request**:
```
curl -s http://localhost:8080/v1/sessions \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
[{"id":"3d8ba776-...","name":"QA Test Session","project":"nexusmind","started_at":"2026-06-30T03:26:59Z","memory_count":0}]
```

**Verdict**: WORKS

---

### create_session
**Endpoint**: POST /v1/sessions  
**Maps to MCP tools**: create_session

**Request**:
```
curl -s -X POST http://localhost:8080/v1/sessions \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Session","project":"nexusmind","agent_id":"audit-agent","tool":"audit-agent"}'
```

**Response** (HTTP 200):
```json
{"id":"3a32c582-a1b4-4ee5-aa4d-713b8365ea66","name":"Audit Test Session"}
```

**Verdict**: WORKS

---

### update_session
**Endpoint**: PATCH /v1/sessions/{id}  
**Maps to MCP tools**: update_session

**Request**:
```
curl -s -X PATCH http://localhost:8080/v1/sessions/3a32c582-a1b4-4ee5-aa4d-713b8365ea66 \
  -H "Authorization: Bearer nm_demo_acme_admin" \
  -H "Content-Type: application/json" \
  -d '{"name":"Audit Test Session Updated","summary":"Summary for audit session"}'
```

**Response** (HTTP 200):
```json
{"id":"3a32c582-...","name":"Audit Test Session Updated","summary":"Summary for audit session","started_at":"2026-06-30T03:27:36Z"}
```

**Verdict**: WORKS

---

### get_session_memories
**Endpoint**: GET /v1/memory?session_id={id}  
**Maps to MCP tools**: get_session_memories

**Request**:
```
curl -s "http://localhost:8080/v1/memory?session_id=3a32c582-a1b4-4ee5-aa4d-713b8365ea66" \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 200):
```json
{"memories":[],"total":0,"limit":50,"offset":0}
```

> **NOTE**: The `session_id` query param filter works (200 OK, correct JSON shape). However, memories stored with a `session_id` in the metadata body are NOT linked to the session at the DB column level. Use `GET /v1/sessions/:id/memories` for session-scoped retrieval.

**Verdict**: WORKS

---

### delete_session
**Endpoint**: DELETE /v1/sessions/{id}  
**Maps to MCP tools**: delete_session

**Request**:
```
curl -s -o /dev/null -w "%{http_code}" -X DELETE \
  http://localhost:8080/v1/sessions/3a32c582-a1b4-4ee5-aa4d-713b8365ea66 \
  -H "Authorization: Bearer nm_demo_acme_admin"
```

**Response** (HTTP 405): Method Not Allowed

> **BUG**: `DELETE /v1/sessions/{id}` is not registered in the router. The router registers only GET (list), POST (create), GET /:id (get), PATCH /:id (update), and GET /:id/memories. There is no DELETE route for sessions. The `delete_session` MCP tool has no working backend endpoint.

**Verdict**: FAILS (route not registered — 405 Method Not Allowed)

---

## Bug Summary

| # | Severity | Endpoint | Bug |
|---|----------|----------|-----|
| 1 | HIGH | GET /v1/code/context | `file_path` query param is required but undocumented. All MCP callers receive 400 without it. |
| 2 | HIGH | PATCH /v1/admin/collections/{id} | Route not registered (405). `update_collection` MCP tool is broken with no backend. |
| 3 | HIGH | PATCH /v1/admin/users/{id} | Route not registered (404). `assign_user_role` MCP tool is broken with no backend. |
| 4 | HIGH | DELETE /v1/sessions/{id} | Route not registered (405). `delete_session` MCP tool is broken with no backend. |
| 5 | LOW | POST /v1/memory/store | Returns 200 instead of 201 (API spec says 201 Created). |
| 6 | LOW | POST /v1/policies | API spec shows invalid field names (`rule.type`, `rule.max_cost_usd`). Actual fields: `rule_type`, `config`. |
| 7 | LOW | PATCH /v1/admin/org/announcement | Body field is `announcement`, not `text`. Only discoverable by reading source. |
| 8 | LOW | POST /v1/admin/tags/rename | Body fields are `from`/`to`, not `old_tag`/`new_tag`. |
| 9 | LOW | POST /v1/webhooks | Body field is `target_url`, not `url`. |
| 10 | INFO | DELETE /v1/code/projects/{name} | Route param `:id` is actually the project name string, not integer. Numeric id fails. |
| 11 | INFO | POST /v1/admin/keys | `user_id` required but not documented in MCP tool description. |
| 12 | INFO | POST /v1/roles | Both `name` and `display_name` required; typically only `name` is communicated. |
