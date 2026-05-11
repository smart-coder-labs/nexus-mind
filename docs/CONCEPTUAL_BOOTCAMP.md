# 🧠 NexusMind Conceptual Bootcamp

> Tu mapa de estudio para dominar todos los temas necesarios para construir NexusMind.
> Cada tema tiene recursos, papers, libros, cursos y proyectos de referencia.

**Versión**: 1.0 — Mayo 2026
**Perfil**: Cesar Ruiz — Full-stack + AI, construyendo un control plane universal para AI coding tools

---

## 📊 Progreso General

```
[████████░░░░░░░░░░░░] 40% — 6 de 12 temas iniciados
```

---

# TEMA 1: Rust Avanzado ⚙️

El core de NexusMind está escrito en Rust. Necesitas dominar async, traits, FFI, y el ecosistema de herramientas.

## 1.1 Async Rust & Tokio

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| async/await fundamentals | P0 | 2h | [ ] |
| Tokio runtime: tasks, spawn, blocking | P0 | 4h | [ ] |
| Async streams & channels | P0 | 2h | [ ] |
| Cancellation & graceful shutdown | P0 | 2h | [ ] |
| Pin, Unpin, and Futures | P1 | 3h | [ ] |
| Async testing with tokio::test | P0 | 1h | [ ] |

**Papers**:
- ["Async Rust: A Comprehensive Guide"](https://rust-lang.github.io/async-book/) — Rust Team (2023) — 📖 **Lectura obligatoria**
- ["Tokio: An Asynchronous Runtime for Rust"](https://tokio.rs) — Carl Lerche (2023)
- ["Why async Rust?"](https://without.boats/blog/why-async-rust/) — without.boats (2023)

**Libros**:
- *Programming Rust (2nd Ed)* — Jim Blandy, Jason Orendorff — Capítulos 19-21
- *Rust for Rustaceans* — Jon Gjengset — Capítulo 7 (Async)

**Cursos**:
- [Tokio Tutorial Oficial](https://tokio.rs/tokio/tutorial) — Gratis, 4h
- [Jon Gjengset: "Understanding Async Rust"](https://www.youtube.com/watch?v=zJHiLm0cC18) — YouTube, 2h
- [Rust Async Book](https://rust-lang.github.io/async-book/) — Gratis

**Repos referencia**:
- [tokio-rs/tokio](https://github.com/tokio-rs/tokio) — Runtime oficial
- [replibyte](https://github.com/Qovery/replibyte) — Replicación en Rust con Tokio
- [meilisearch](https://github.com/meilisearch/meilisearch) — Search engine en Rust con Tokio

**Proyectos que implementan conceptos similares**:
- [Engram](https://github.com/Gentleman-Programming/Engram) — Referencia directa (Go, pero el patrón importa)
- [tabby](https://github.com/TabbyML/tabby) — AI coding assistant, Rust + Tokio

## 1.2 Frameworks Web: Axum

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Router, handlers, extractors | P0 | 2h | [ ] |
| State management & shared state | P0 | 1h | [ ] |
| Middleware (auth, logging, CORS) | P0 | 2h | [ ] |
| Tower service stack | P0 | 2h | [ ] |
| Error handling in handlers | P0 | 1h | [ ] |
| OpenAPI generation (utoipa/aide) | P1 | 2h | [ ] |
| SSE & WebSockets | P1 | 2h | [ ] |
| Multipart file upload | P1 | 1h | [ ] |
| Rate limiting & throttling | P0 | 1h | [ ] |

**Papers/Lecturas**:
- [Axum Docs](https://docs.rs/axum/latest/axum/) — Guía oficial
- ["Tower: A Library for Resilient Network Services"](https://github.com/tower-rs/tower) — Tower team

**Cursos**:
- [Axum Workshop](https://github.com/tokio-rs/axum/tree/main/examples) — Ejemplos oficiales
- [Zero to Production in Rust](https://www.zero2prod.com) — Luca Palmieri — Axum + Postgres

**Repos referencia**:
- [tokio-rs/axum](https://github.com/tokio-rs/axum) — Framework oficial
- [shuttle](https://github.com/shuttle-hq/shuttle) — Deploy de apps Rust con Axum
- [loco](https://github.com/loco-rs/loco) — Rails-like framework en Rust con Axum

## 1.3 ORM & Database Drivers

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| rusqlite: conexión, queries, statements | P0 | 3h | [ ] |
| rusqlite: bundled mode, compile features | P0 | 1h | [ ] |
| sqlx: compile-time checked queries | P0 | 4h | [ ] |
| sqlx: migrations, pooling, transactions | P0 | 3h | [ ] |
| Serde integration (serde_json, rmp-serde) | P0 | 2h | [ ] |
| Custom types & NULL handling | P0 | 1h | [ ] |
| Batch operations & transactions | P0 | 2h | [ ] |

**Papers**:
- ["SQLite Documentation"](https://sqlite.org/docs.html) — D. Richard Hipp (2024) — **LEER**: WAL mode, FTS5, locking

**Libros**:
- *Rust in Action* — Tim McNamara — Capítulo 10 (Databases)
- *Zero to Production in Rust* — Luca Palmieri — Capítulos 7-9 (sqlx)

**Cursos**:
- [sqlx getting started](https://github.com/launchbadge/sqlx#usage) — GitHub README
- [Zero2Prod Database Chapters](https://www.zero2prod.com) — Luca Palmieri

**Repos referencia**:
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite bindings
- [launchbadge/sqlx](https://github.com/launchbadge/sqlx) — Postgres/MySQL/SQLite async driver
- [diesel](https://github.com/diesel-rs/diesel) — Alternative ORM (no recomendado para sync-heavy)

## 1.4 Módulos Core de NexusMind

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Trait objects vs generics (MemoryStore trait) | P0 | 2h | [ ] |
| Error handling with thiserror + anyhow | P0 | 2h | [ ] |
| Builder pattern for config | P0 | 1h | [ ] |
| Type state pattern (auth flows) | P1 | 2h | [ ] |
| Newtype pattern for IDs | P0 | 1h | [ ] |
| PhantomData & marker types | P1 | 2h | [ ] |
| Associated type constructors | P2 | 2h | [ ] |

**Papers**:
- ["Rust Design Patterns"](https://rust-unofficial.github.io/patterns/) — Rust Community

**Libros**:
- *Rust for Rustaceans* — Jon Gjengset — Capítulo 2 (Types), Capítulo 3 (Traits)

---

# TEMA 2: Bases de Datos 🗄️

El corazón de NexusMind. SQLite local + Postgres cloud + sync engine.

## 2.1 SQLite Internals

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Architecture: B-tree, pager, VFS | P0 | 4h | [ ] |
| WAL mode vs journal mode | P0 | 2h | [ ] |
| Locking & concurrency | P0 | 2h | [ ] |
| FTS5: tokenizers, queries, ranking | P0 | 4h | [ ] |
| sqlite-vec: vectors, IVF-PQ | P0 | 3h | [ ] |
| Transactions: deferred, immediate, exclusive | P0 | 2h | [ ] |
| Backup & restore | P0 | 1h | [ ] |
| Performance tuning: page size, cache, mmap | P0 | 2h | [ ] |

**Papers**:
- ["SQLite As An Application File Format"](https://sqlite.org/appfileformat.html) — D. Richard Hipp (2024)
- ["The SQLite FTS5 Extension"](https://sqlite.org/fts5.html) — Official docs
- ["Better performance with WAL mode"](https://sqlite.org/wal.html) — Oficial

**Libros**:
- *Using SQLite* — Jay Kreibich — O'Reilly, **la biblia de SQLite**
- *The Definitive Guide to SQLite* — Mike Owens

**Cursos**:
- [SQLite Documentation (complete read)](https://sqlite.org/docs.html) — 8h de lectura técnica
- [SQLite Performance Tuning](https://www.sqlite.org/speed.html) — Lectura obligatoria

**Repos referencia**:
- [sqlite/sqlite](https://sqlite.org/src) — Código fuente
- [asg017/sqlite-vec](https://github.com/asg017/sqlite-vec) — Vector search extension

## 2.2 Postgres Internals

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Architecture: processes, shared buffers, WAL | P0 | 4h | [ ] |
| MVCC: transaction IDs, snapshots, visibility | P0 | 3h | [ ] |
| Indexes: B-tree, GiST, GIN, BRIN | P0 | 3h | [ ] |
| pgvector: HNSW, IVFFlat | P0 | 3h | [ ] |
| tsvector & tsquery (FTS) | P0 | 3h | [ ] |
| Row-Level Security (RLS) | P0 | 2h | [ ] |
| Connection pooling (PgBouncer) | P0 | 2h | [ ] |
| Replication: streaming, logical | P1 | 3h | [ ] |
| Partitioning, sharding | P1 | 2h | [ ] |

**Papers**:
- ["The Internals of PostgreSQL"](https://www.interdb.jp/pg/) — Hironobu Suzuki (2024) — **Lectura OBLIGATORIA**
- ["PostgreSQL: The Design of a Next-Generation Database System"](https://dsf.berkeley.edu/papers/ERL-M87-61.pdf) — Stonebraker et al. (1987)
- ["What Goes Around Comes Around"](https://people.cs.umass.edu/~yanlei/courses/CS691LL-f06/papers/STON05.pdf) — Stonebraker (2005) — Evolución de modelos de datos
- [PostgreSQL RLS Documentation](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)

**Libros**:
- *PostgreSQL: Up and Running (3rd Ed)* — Regina Obe, Leo Hsu — O'Reilly
- *PostgreSQL 14 Internals* — Egor Rogov — **La biblia moderna de Postgres**
- *The Art of PostgreSQL* — Dimitri Fontaine

**Cursos**:
- [PostgreSQL Tutorial](https://www.postgresqltutorial.com/) — Gratis
- [Use The Index, Luke](https://use-the-index-luke.com/) — Guía de indexing
- [CMU 15-445: Database Systems](https://15445.courses.cs.cmu.edu/) — Andy Pavlo (grabado en YouTube)

**Repos referencia**:
- [postgres/postgres](https://github.com/postgres/postgres) — Código fuente
- [pgvector/pgvector](https://github.com/pgvector/pgvector) — Vector extension
- [timescaledb](https://github.com/timescale/timescaledb) — Time-series extension (referencia de extensiones)

## 2.3 Replicación & Sincronización

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Logical replication de Postgres | P0 | 3h | [ ] |
| SQLite → Postgres sync strategies | P0 | 4h | [ ] |
| Conflict resolution: LWW, CRDT, operational transform | P0 | 4h | [ ] |
| Offline-first data models | P0 | 3h | [ ] |
| Last-writer-wins con timestamps | P0 | 2h | [ ] |
| CRDT basics (Merkle-Clock, RGA) | P1 | 4h | [ ] |
| Sync queue & retry logic | P0 | 2h | [ ] |

**Papers**:
- ["Conflict-Free Replicated Data Types"](https://hal.inria.fr/inria-00609399v1/document) — Shapiro et al. (2011) — **CRDT paper fundacional**
- ["Local-First Software"](https://martin.kleppmann.com/papers/local-first.pdf) — Kleppmann et al. (2019) — **Lectura OBLIGATORIA** para el modelo de NexusMind
- ["Merkle-CRDTs"](https://arxiv.org/abs/2004.00107) — Kleppmann et al. (2020)
- ["A Conflict-Free Replicated JSON Datatype"](https://arxiv.org/abs/1608.03960) — Kleppmann, Beresford (2016)

**Libros**:
- *Designing Data-Intensive Applications* — Martin Kleppmann — **CAPÍTULOS 5-9 OBLIGATORIOS** — Replication, Partitioning, Transactions, Consistency

**Cursos**:
- [Martin Kleppmann: CRDTs](https://www.youtube.com/watch?v=E5mRk3LzIhE) — YouTube, 1h
- [CMU 15-440: Distributed Systems](https://www.youtube.com/playlist?list=PL7Rjli3A8rYFNp6MtdUQhFJQjmYffscT2) — Lectures

**Repos referencia**:
- [y-crdt](https://github.com/y-crdt/y-crdt) — CRDT en Rust
- [automerge](https://github.com/automerge/automerge) — CRDT JSON en Rust
- [electric-sql/pglite](https://github.com/electric-sql/pglite) — Postgres local → cloud sync (referencia directa)
- [vlcn-io/cr-sqlite](https://github.com/vlcn-io/cr-sqlite) — CRDT support for SQLite (referencia directa)

## 2.4 Performance & Tuning

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Query planning & EXPLAIN ANALYZE | P0 | 3h | [ ] |
| Index strategies for FTS + vector hybrid | P0 | 3h | [ ] |
| Connection pooling & resource limits | P0 | 2h | [ ] |
| Monitoring: pg_stat_statements, slow queries | P0 | 2h | [ ] |
| Benchmarking tools: pgbench, sqlite-speedtest | P0 | 2h | [ ] |

---

# TEMA 3: Embeddings & Vector Search 🔍

Core de la búsqueda semántica de NexusMind. Búsqueda híbrida (FTS + vectors) con ONNX + Candle.

## 3.1 Embedding Models

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Word embeddings vs sentence embeddings | P0 | 2h | [ ] |
| all-MiniLM-L6-v2: architecture, use cases | P0 | 3h | [ ] |
| Model quantization: ONNX, dynamic int8 | P0 | 3h | [ ] |
| Model deployment with Candle | P0 | 4h | [ ] |
| ONNX Runtime in Rust (ort crate) | P0 | 4h | [ ] |
| Tokenizers: BPE, WordPiece, SentencePiece | P0 | 2h | [ ] |
| Cross-encoders vs bi-encoders | P0 | 2h | [ ] |

**Papers**:
- ["Sentence-BERT: Sentence Embeddings using Siamese BERT-Networks"](https://arxiv.org/abs/1908.10084) — Reimers, Gurevych (2019) — **SBERT, el paper fundacional**
- ["All MiniLM-L6-v2"](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) — HuggingFace Model Card
- ["Efficient Estimation of Word Representations in Vector Space"](https://arxiv.org/abs/1301.3781) — Mikolov et al. (2013) — Word2Vec
- ["ONNX: Open Neural Network Exchange"](https://onnx.ai/) — Microsoft, Facebook (2019)

**Libros**:
- *Speech and Language Processing (3rd Ed)* — Jurafsky, Martin — Capítulos 6-7 (Embeddings)
- *Deep Learning* — Goodfellow, Bengio, Courville — Capítulo 14 (Autoencoders)

**Cursos**:
- [HuggingFace NLP Course](https://huggingface.co/learn/nlp-course) — Gratis, capítulos 2-5
- [Fast.ai Practical Deep Learning](https://course.fast.ai/) — Lecciones 4-8
- [Candle: ML in Rust](https://github.com/huggingface/candle) — Ejemplos + tutoriales

**Repos referencia**:
- [huggingface/candle](https://github.com/huggingface/candle) — ML framework Rust
- [pykeio/ort](https://github.com/pykeio/ort) — ONNX Runtime Rust bindings
- [sentence-transformers](https://github.com/UKPLab/sentence-transformers) — SBERT Python (referencia para testing)

## 3.2 Similarity & Search Algorithms

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Cosine similarity vs dot product vs euclidean | P0 | 2h | [ ] |
| HNSW: Hierarchical Navigable Small World | P0 | 4h | [ ] |
| IVF-PQ: Inverted File with Product Quantization | P0 | 3h | [ ] |
| Flat search (brute force) | P0 | 1h | [ ] |
| Vector quantization: scalar, product, binary | P0 | 3h | [ ] |
| Recall vs latency tradeoffs | P0 | 2h | [ ] |

**Papers**:
- ["Efficient and Robust Approximate Nearest Neighbor Search using Hierarchical Navigable Small World Graphs"](https://arxiv.org/abs/1603.09320) — Malkov, Yashunin (2016) — **HNSW paper fundacional**
- ["Product Quantization for Nearest Neighbor Search"](https://hal.inria.fr/inria-00514412v2/document) — Jégou et al. (2010) — **PQ paper**
- ["Billion-scale similarity search with GPUs"](https://arxiv.org/abs/1702.08734) — Johnson, Douze, Jégou (2017) — FAISS

**Repos referencia**:
- [facebookresearch/faiss](https://github.com/facebookresearch/faiss) — Meta, GPU-accelerated vector search
- [spotify/annoy](https://github.com/spotify/annoy) — Approximate nearest neighbors (C++ reference)
- [nmslib/hnswlib](https://github.com/nmslib/hnswlib) — HNSW implementation (referencia C++)

## 3.3 Hybrid Search (FTS + Vectors)

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Score fusion: weighted sum, RRF | P0 | 3h | [ ] |
| Reciprocal Rank Fusion (RRF) | P0 | 2h | [ ] |
| BM25 + cosine hybrid in SQLite | P0 | 4h | [ ] |
| tsvector + pgvector hybrid in Postgres | P0 | 3h | [ ] |
| Re-ranking with cross-encoders | P0 | 2h | [ ] |
| Query expansion for better recall | P1 | 2h | [ ] |

**Papers**:
- ["Combining Lexical and Semantic Search"](https://www.elastic.co/guide/en/elasticsearch/reference/current/knn-search.html) — Elastic (2024) — Hybrid search patterns
- ["From Distillation to Hard Negative Sampling: Making Sparse Neural IR Models More Effective"](https://arxiv.org/abs/2205.05633) — Formal et al. (2022)
- ["Simple Hybrid Search with Reciprocal Rank Fusion"](https://www.elastic.co/blog/reciprocal-rank-fusion) — Elastic (2024)
- ["Late Interaction with ColBERT"](https://arxiv.org/abs/2004.12832) — Khattab, Zaharia (2020) — Re-ranking avanzado

---

# TEMA 4: Auth & Identity 🔐

Sistema de autenticación zero-trust de NexusMind. SSO, RBAC/ABAC, device fingerprinting.

## 4.1 OIDC & OAuth2

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| OAuth2 roles: resource owner, client, auth server, RS | P0 | 2h | [ ] |
| Authorization Code flow + PKCE | P0 | 3h | [ ] |
| Client Credentials flow (M2M) | P0 | 1h | [ ] |
| OIDC: ID Token, UserInfo endpoint, scopes | P0 | 3h | [ ] |
| Token refresh & rotation | P0 | 2h | [ ] |
| Token revocation | P0 | 1h | [ ] |
| JWT structure, signing algorithms, validation | P0 | 3h | [ ] |
| JWKS: public key rotation | P0 | 1h | [ ] |

**Papers**:
- [RFC 6749: The OAuth 2.0 Authorization Framework](https://datatracker.ietf.org/doc/html/rfc6749) — **LEER**
- [RFC 7636: PKCE](https://datatracker.ietf.org/doc/html/rfc7636)
- [RFC 7519: JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519)
- [OIDC Core Spec](https://openid.net/specs/openid-connect-core-1_0.html)

**Libros**:
- *OAuth 2 in Action* — Justin Richer, Antonio Sanso — Manning
- *OAuth 2.0 Simplified* — Aaron Parecki — **La guía práctica, corta y directa**

**Cursos**:
- [OAuth 2.0 Tutorial](https://oauth.net/2/) — oauth.net
- [Okta Developer](https://developer.okta.com/) — Tutoriales prácticos
- [Auth0 Learn](https://auth0.com/learn) — Cursos gratis

**Repos referencia**:
- [oxidecomputer/hubris](https://github.com/oxidecomputer/hubris) — Auth system en Rust
- [casbin/casbin-rs](https://github.com/casbin/casbin-rs) — Policy engine (RBAC/ABAC) en Rust
- [zitadel](https://github.com/zitadel/zitadel) — Identity platform open source (Go, ref de arquitectura)
- [ory/hydra](https://github.com/ory/hydra) — OAuth2 server (Go, referencia)

## 4.2 SAML & SCIM

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| SAML2: SP-initiated SSO, IdP-initiated SSO | P0 | 3h | [ ] |
| SAML assertions, attributes, NameID | P0 | 2h | [ ] |
| SAML metadata exchange | P0 | 1h | [ ] |
| SCIM 2.0: Users, Groups, schemas | P0 | 3h | [ ] |
| SCIM provisioning, deprovisioning, sync | P0 | 2h | [ ] |
| SAML vs OIDC: cuándo cada uno | P0 | 1h | [ ] |

**Papers**:
- [SAML V2.0 Standard](https://docs.oasis-open.org/security/saml/v2.0/saml-core-2.0-os.pdf) — OASIS (2005)
- [RFC 7643: SCIM Core Schema](https://datatracker.ietf.org/doc/html/rfc7643)
- [RFC 7644: SCIM Protocol](https://datatracker.ietf.org/doc/html/rfc7644)

**Repos referencia**:
- [saml-rs](https://github.com/ruuda/saml-rs) — SAML toolkit en Rust
- [samlify](https://github.com/tngan/samlify) — SAML library JS (referencia funcional)

## 4.3 RBAC / ABAC

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| RBAC: roles, permissions, hierarchies | P0 | 2h | [ ] |
| ABAC: attributes, policies, evaluation engine | P0 | 4h | [ ] |
| RBAC/ABAC hybrid model | P0 | 3h | [ ] |
| Policy evaluation engine design | P0 | 4h | [ ] |
| Role inheritance & conflict resolution | P0 | 2h | [ ] |
| Policy as code (YAML/Rego) | P0 | 3h | [ ] |
| Per-project overrides for roles | P0 | 2h | [ ] |

**Papers**:
- ["Role-Based Access Controls"](https://csrc.nist.gov/CSRC/media/Publications/conference-paper/1992/10/13/15th-national-computer-security-conference/documents/1992-ferraiolo-kuhn.pdf) — Ferraiolo, Kuhn (1992) — **RBAC paper original del NIST**
- [NIST RBAC Standard (ANSI INCITS 359-2012)](https://www.nist.gov/itl/topics/role-based-access-control)
- ["Attribute-Based Access Control"](https://nvlpubs.nist.gov/nistpubs/Legacy/IR/nistir7208.pdf) — Hu et al., NIST (2006) — ABAC paper fundacional
- ["The Relationship Between RBAC and ABAC"](https://csrc.nist.rip/groups/SNS/rbac/documents/ABAC_RBAC.pdf) — Kuhn, Coyne, Weil (2010)

**Repos referencia**:
- [casbin/casbin-rs](https://github.com/casbin/casbin-rs) — Policy engine en Rust
- [open-policy-agent/opa](https://github.com/open-policy-agent/opa) — OPA (Rego policy engine, referencia de arquitectura)
- [cerbos](https://github.com/cerbos/cerbos) — Access control engine (Go, referencia)
- [osohq/oso](https://github.com/osohq/oso) — Policy engine (Polar language, Rust)

## 4.4 Zero Trust & Device Identity

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Zero Trust: never trust, always verify | P0 | 2h | [ ] |
| Device fingerprinting (browser, hardware, network) | P0 | 3h | [ ] |
| Session binding: user + device + tool | P0 | 2h | [ ] |
| WebAuthn / passkeys | P0 | 3h | [ ] |
| MFA: TOTP, SMS, biometric | P0 | 2h | [ ] |
| Continuous verification & token lifetime | P0 | 2h | [ ] |
| Tool identity (separate from user identity) | P0 | 3h | [ ] |

**Papers**:
- ["Zero Trust Architecture"](https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-207.pdf) — NIST SP 800-207 (2020) — **Documento fundacional de Zero Trust**
- ["WebAuthn Specification"](https://www.w3.org/TR/webauthn-3/) — W3C (2019)
- ["BeyondCorp: A New Approach to Enterprise Security"](https://research.google/pubs/pub43231/) — Google (2014)
- ["Device Fingerprinting"](https://petsymposium.org/2012/papers/hotpets12-5.pdf) — Eckersley (2012) — Panopticlick, EFF

**Repos referencia**:
- [webauthn-rs](https://github.com/kanidm/webauthn-rs) — WebAuthn en Rust
- [kanidm/kanidm](https://github.com/kanidm/kanidm) — Identity manager en Rust (referencia directa)
- [fingerprintjs](https://github.com/fingerprintjs/fingerprintjs) — Device fingerprinting JS (referencia)

---

# TEMA 5: Protocolos AI 🤖

MCP, ACP, tool calling — cómo NexusMind se conecta con Claude Code, Cursor, y otros tools.

## 5.1 MCP — Model Context Protocol

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| MCP architecture: host, client, server | P0 | 3h | [ ] |
| Protocol: JSON-RPC 2.0, transport (stdio, SSE) | P0 | 3h | [ ] |
| Resources: discovery, reading, subscriptions | P0 | 2h | [ ] |
| Tools: expose, call, result handling | P0 | 3h | [ ] |
| Prompts: templates, dynamic | P0 | 2h | [ ] |
| Sampling: LLM calls from server | P0 | 1h | [ ] |
| Auth in MCP: OAuth flow | P0 | 2h | [ ] |
| MCP vs ACP (Agent Communication Protocol) | P0 | 1h | [ ] |

**Papers**:
- [MCP Specification](https://spec.modelcontextprotocol.io/) — Anthropic — **LEER COMPLETO**
- [MCP GitHub](https://github.com/modelcontextprotocol/servers) — Referencia de servidores
- [Anthropic MCP Docs](https://docs.anthropic.com/en/docs/agents-and-tools/mcp) — Guía oficial

**Repos referencia**:
- [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers) — MCP servers de referencia
- [mcp-rs](https://github.com/rust-mcp/mcp-rs) — MCP en Rust (crear si no existe)
- [cline/mcp](https://github.com/nicholasgriffintn/mcp-server) — MCP servers community

## 5.2 Agent Orchestration

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Multi-agent architectures | P0 | 3h | [ ] |
| Tool use patterns: describe, call, observe | P0 | 2h | [ ] |
| Agent loops: plan, execute, reflect | P0 | 2h | [ ] |
| Memory for agents: context window management | P0 | 3h | [ ] |
| Human-in-the-loop patterns | P0 | 2h | [ ] |
| Orchestrator vs swarm patterns | P0 | 2h | [ ] |

**Papers**:
- ["Tool Use in LLMs"](https://arxiv.org/abs/2306.08302) — Schick et al. (2023) — **Toolformer, paper fundacional**
- ["ReAct: Synergizing Reasoning and Acting in Language Models"](https://arxiv.org/abs/2210.03629) — Yao et al. (2022)
- ["Reflexion: Language Agents with Verbal Reinforcement Learning"](https://arxiv.org/abs/2303.11366) — Shinn et al. (2023)

**Repos referencia**:
- [anthropics/claude-code](https://github.com/anthropics/claude-code) — Claude Code
- [anthropics/claude-code-memory](https://github.com/anthropics/claude-code-memory) — Memory MCP server
- [openai/codex](https://github.com/openai/codex) — OpenAI Codex CLI

---

# TEMA 6: Criptografía Aplicada 🔏

Audit trail con árboles de Merkle, firmas Ed25519, verificación criptográfica.

## 6.1 Hashing & Merkle Trees

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| SHA-256: properties, collisions, usage | P0 | 2h | [ ] |
| Merkle trees: construction, verification | P0 | 3h | [ ] |
| Merkle audit proof (inclusion proof) | P0 | 2h | [ ] |
| Incremental hashing & partial trees | P0 | 2h | [ ] |
| Timestamping & chain linking | P0 | 2h | [ ] |

**Papers**:
- ["Merkle Tree Overview"](https://en.wikipedia.org/wiki/Merkle_tree) — Ralph Merkle (1979) — **Patente original**
- ["Certificate Transparency"](https://certificate.transparency.dev/) — Google — Merkle trees en producción
- ["RFC 6962: Certificate Transparency"](https://datatracker.ietf.org/doc/html/rfc6962) — Google (2013)

## 6.2 Digital Signatures

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Ed25519: properties, performance, security | P0 | 3h | [ ] |
| Public key cryptography basics | P0 | 2h | [ ] |
| Signing & verification in Rust | P0 | 2h | [ ] |
| Key management: generation, storage, rotation | P0 | 3h | [ ] |
| HSM integration for enterprise | P1 | 2h | [ ] |

**Papers**:
- ["Ed25519: High-Speed High-Security Signatures"](https://ed25519.cr.yp.to/) — Bernstein et al. (2011)
- ["RFC 8032: EdDSA"](https://datatracker.ietf.org/doc/html/rfc8032) — RFC (2017)

**Repos referencia**:
- [dalek-cryptography/ed25519-dalek](https://github.com/dalek-cryptography/ed25519-dalek) — Ed25519 en Rust
- [rustls/rustls](https://github.com/rustls/rustls) — TLS en Rust

---

# TEMA 7: Arquitectura de Software 🏗️

Patrones de diseño, ADRs, CQRS, hexagonal architecture.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| ADRs: Architecture Decision Records | P0 | 2h | [ ] |
| CQRS

# TEMA 7: Arquitectura de Software 🏗️ (cont.)

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| ADRs: Architecture Decision Records | P0 | 2h | [ ] |
| CQRS: Command Query Responsibility Segregation | P0 | 3h | [ ] |
| Event Sourcing: events as source of truth | P0 | 4h | [ ] |
| Hexagonal architecture / Ports & Adapters | P0 | 3h | [ ] |
| Trait abstractions (MemoryStore, PolicyStore) | P0 | 2h | [ ] |
| Repository pattern in Rust | P0 | 2h | [ ] |
| Dependency injection without DI framework | P0 | 2h | [ ] |
| DDD: bounded contexts, aggregates, domain events | P1 | 4h | [ ] |
| Error handling strategies: recoverable vs fatal | P0 | 2h | [ ] |

**Papers**:
- ["Documenting Architecture Decisions"](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) — Michael Nygard (2011) — **ADR original**
- ["CQRS Documents"](https://cqrs.nu/) — Greg Young
- ["Hexagonal Architecture"](https://alistair.cockburn.us/hexagonal-architecture/) — Alistair Cockburn (2005)
- ["Domain-Driven Design Reference"](https://domainlanguage.com/ddd/reference/) — Eric Evans

**Libros**:
- *Domain-Driven Design* — Eric Evans — **La biblia de DDD**
- *Implementing Domain-Driven Design* — Vaughn Vernon
- *Clean Architecture* — Robert C. Martin
- *Building Evolutionary Architectures* — Neal Ford, Rebecca Parsons, Patrick Kua

**Cursos**:
- [DDD Quickly](https://www.domainlanguage.com/ddd/) — Resumen gratuito de DDD
- [Architecture: The Hard Parts](https://www.oreilly.com/library/view/software-architecture-the/9781492086895/) — Neal Ford (O'Reilly)

**Repos referencia**:
- [rust-unofficial/patterns](https://github.com/rust-unofficial/patterns) — Rust design patterns
- [eventstore](https://github.com/EventStore/EventStore) — Event Store (referencia arquitectura)
- [thalo](https://github.com/thalo-rs/thalo) — Event sourcing framework en Rust

---

# TEMA 8: Sistemas Distribuidos 🌐

Replicación, consistencia, consensus — fundamentos del sync engine.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| CAP theorem: consistency, availability, partition tolerance | P0 | 3h | [ ] |
| PACELC: latency vs consistency tradeoff | P0 | 2h | [ ] |
| Consistency models: strong, eventual, causal | P0 | 3h | [ ] |
| Vector clocks & Lamport timestamps | P0 | 3h | [ ] |
| Conflict-free Replicated Data Types (CRDTs) | P0 | 4h | [ ] |
| Operational Transform (OT) | P1 | 3h | [ ] |
| Raft consensus algorithm | P1 | 4h | [ ] |
| Gossip protocols | P1 | 2h | [ ] |
| Distributed transactions (2PC, Saga) | P1 | 3h | [ ] |

**Papers**:
- ["Brewer's Conjecture and the Feasibility of Consistent, Available, Partition-Tolerant Web Services"](https://dl.acm.org/doi/10.1145/564585.564601) — Gilbert, Lynch (2002) — **CAP Theorem**
- ["Conflict-Free Replicated Data Types"](https://hal.inria.fr/inria-00609399v1/document) — Shapiro et al. (2011) — **CRDTs**
- ["In Search of an Understandable Consensus Algorithm"](https://raft.github.io/raft.pdf) — Ongaro, Ousterhout (2014) — **Raft**
- ["Dynamo: Amazon's Highly Available Key-value Store"](https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf) — DeCandia et al. (2007)
- ["Calvin: Fast Distributed Transactions for Partitioned Database Systems"](https://cs.yale.edu/homes/thomson/publications/calvin-sigmod12.pdf) — Thomson et al. (2012)
- ["Time, Clocks, and the Ordering of Events in a Distributed System"](https://lamport.azurewebsites.net/pubs/time-clocks.pdf) — Leslie Lamport (1978) — **Fundacional**

**Libros**:
- *Designing Data-Intensive Applications* — Martin Kleppmann — **OBLIGATORIO COMPLETO**
- *Distributed Systems (3rd Ed)* — Maarten van Steen, Andrew Tanenbaum
- *Understanding Distributed Systems* — Roberto Vitillo

**Cursos**:
- [MIT 6.824: Distributed Systems](https://pdos.csail.mit.edu/6.824/) — **El mejor curso de sistemas distribuidos del mundo**
- [Martin Kleppmann: Distributed Systems lectures](https://www.youtube.com/playlist?list=PLeKd45zvjcDFUEv_ohr_HdUFe97RItdiB)
- [CMU 15-440: Distributed Systems](https://www.cs.cmu.edu/~dga/15-440/S14/)

**Repos referencia**:
- [tikv/tikv](https://github.com/tikv/tikv) — Distributed KV store en Rust (Raft)
- [etcd-io/etcd](https://github.com/etcd/etcd) — Distributed KV (Raft, Go, ref)
- [sled](https://github.com/spacejam/sled) — Embedded database in Rust (lock-free)
- [materialize](https://github.com/MaterializeInc/materialize) — Streaming SQL database in Rust

---

# TEMA 9: Enterprise Security 🛡️

SOC2, GDPR, compliance — lo que NexusMind necesita para vender a empresas.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| SOC 2 Type I & II: trust services criteria | P0 | 4h | [ ] |
| GDPR: data subject rights, consent, DPA | P0 | 3h | [ ] |
| Data residency: regional storage requirements | P0 | 2h | [ ] |
| Encryption at rest: AES, key wrapping | P0 | 2h | [ ] |
| Encryption in transit: TLS 1.3, mTLS | P0 | 2h | [ ] |
| Key management: HSM, KMS, rotation policies | P0 | 3h | [ ] |
| Audit logging: immutability, retention | P0 | 2h | [ ] |
| Incident response: detection, containment, recovery | P0 | 2h | [ ] |
| Penetration testing methodology | P0 | 2h | [ ] |
| SBOM: software bill of materials | P0 | 1h | [ ] |

**Papers/Standards**:
- [SOC 2 Framework (AICPA)](https://www.aicpa-cima.com/topic/audit-assurance/audit-and-assurance-guidance-soc-2) — AICPA
- [GDPR Full Text](https://gdpr-info.eu/) — EU
- [NIST SP 800-53: Security and Privacy Controls](https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/) — Web application security risks

**Libros**:
- *The Security Auditor's Guide to SOC 2* — Mike Herzog
- *GDPR: A Practical Guide* — IT Governance Privacy Team
- *Threat Modeling: Designing for Security* — Adam Shostack

**Cursos**:
- [OWASP Web Security Testing Guide](https://owasp.org/www-project-web-security-testing-guide/)
- [SANS SEC401: Security Essentials](https://www.sans.org/cyber-security-courses/security-essentials/)

---

# TEMA 10: DevOps & Infra 🚀

Build, deploy, run. Supabase, Docker, ARM, CI/CD.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Supabase: tables, RLS, auth, realtime | P0 | 4h | [ ] |
| Docker: multi-stage builds, distroless | P0 | 3h | [ ] |
| Docker Compose: local development stack | P0 | 2h | [ ] |
| ARM64 cross-compilation for Rust | P0 | 3h | [ ] |
| CI/CD: GitHub Actions, caching, releases | P0 | 3h | [ ] |
| Observability: OpenTelemetry, tracing, metrics | P0 | 3h | [ ] |
| Structured logging (tracing crate) | P0 | 2h | [ ] |
| Health checks, readiness probes | P0 | 1h | [ ] |
| Benchmarks & CI performance gates | P0 | 2h | [ ] |

**Libros**:
- *Docker Deep Dive* — Nigel Poulton
- *Observability Engineering* — Charity Majors, Liz Fong-Jones, George Miranda
- *The Site Reliability Workbook* — Google SRE Team

**Cursos**:
- [Docker & Kubernetes: The Practical Guide](https://www.udemy.com/course/docker-kubernetes-the-practical-guide/) — Udemy
- [Supabase Docs](https://supabase.com/docs) — Documentación oficial
- [GitHub Actions Docs](https://docs.github.com/en/actions)

**Repos referencia**:
- [supabase/supabase](https://github.com/supabase/supabase) — Firebase alternative
- [cross-rs/cross](https://github.com/cross-rs/cross) — Cross-compilation Rust
- [tracing-rs/tracing](https://github.com/tokio-rs/tracing) — Logging + tracing

---

# TEMA 11: Marketing Técnico & DevRel 📢

Developer relations, open source strategy, go-to-market.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Developer persona: who uses NexusMind | P0 | 2h | [ ] |
| Open source strategy: license, governance | P0 | 2h | [ ] |
| Documentation: API docs, guides, tutorials | P0 | 3h | [ ] |
| Landing page conversion: CTAs, social proof | P0 | 2h | [ ] |
| Pricing strategy: open source + enterprise | P0 | 3h | [ ] |
| Enterprise sales cycle: POC, pilots, procurement | P0 | 2h | [ ] |
| DevRel: conferences, meetups, content | P0 | 2h | [ ] |
| Competitive positioning (vs Engram, etc.) | P0 | 2h | [ ] |

**Libros**:
- *Developer Marketing: The Essential Guide* — The DevRel Collective
- *Working in Public: The Making and Maintenance of Open Source Software* — Nadia Eghbal
- *Obviously Awesome* — April Dunford (positioning)

**Recursos**:
- [DevRel Weekly](https://devrelweekly.com/) — Newsletter
- [Open Source Guides](https://opensource.guide/) — GitHub
- [Vercel's Open Source Playbook](https://vercel.com/guides/oss-playbook) — Vercel

---

# TEMA 12: AI/LLMs — RAG & Agent Patterns 🧠

Context windows, tool use, agent memory patterns.

| Subtema | Prioridad | Tiempo | Check |
|---------|-----------|--------|-------|
| Context window: token counting, truncation, sliding window | P0 | 2h | [ ] |
| RAG: chunking, retrieval, generation | P0 | 4h | [ ] |
| Context management strategies | P0 | 3h | [ ] |
| Prompt engineering for memory retrieval | P0 | 2h | [ ] |
| Tool calling: function schemas, parameter extraction | P0 | 3h | [ ] |
| Agent loop: observe, plan, act, reflect | P0 | 3h | [ ] |
| Memory for agents: episodic, semantic, procedural | P0 | 2h | [ ] |
| Token cost optimization | P0 | 2h | [ ] |
| Model routing: small vs large models | P0 | 2h | [ ] |

**Papers**:
- ["Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks"](https://arxiv.org/abs/2005.11401) — Lewis et al. (2020) — **RAG paper fundacional**
- ["Lost in the Middle: How Language Models Use Long Contexts"](https://arxiv.org/abs/2307.03172) — Liu et al. (2023) — **Por qué RAG importa**
- ["ReAct: Synergizing Reasoning and Acting in Language Models"](https://arxiv.org/abs/2210.03629) — Yao et al. (2022)
- ["Gemini 1.5: Long Context Breakthrough"](https://arxiv.org/abs/2403.05530) — Google (2024)
- ["Toolformer: Language Models Can Teach Themselves to Use Tools"](https://arxiv.org/abs/2302.04761) — Schick et al. (2023)

**Libros**:
- *Building LLM Applications* — Valentina Alto
- *LLM Engineer's Handbook* — Paul Iusztin, Maxime Labonne
- *Designing Machine Learning Systems* — Chip Huyen

**Cursos**:
- [Anthropic Prompt Engineering Guide](https://docs.anthropic.com/en/docs/build-with-claude/prompt-engineering)
- [OpenAI Platform Docs](https://platform.openai.com/docs/guides/prompt-engineering)
- [LangChain Academy](https://python.langchain.com/v0.2/docs/tutorials/)

---

# 🗺️ Roadmap de Estudio Acelerado (4 Semanas ⚡)

> **Contenido idéntico al de 8 semanas, pero con 30h/semana (6días × 5h/día).**
> Días intensivos: cada día cuenta. Domingos de repaso/descanso.

## 📅 Calendario General

```
Semana 1 | Fundamentos (Rust + DBs)
Semana 2 | Core (Auth + Crypto + MCP)
Semana 3 | Avanzado (Vectors + Distributed + Sync)
Semana 4 | Empresa (Security + DevOps + Marketing)
```

---

## Semana 1 — Fundamentos (Rust + Databases)

**Objetivo**: Escribir Rust async productivo y entender SQLite/Postgres a nivel interno.

### Día 1: Async Rust intensivo
```
Mañana (3h):
  • Rust Async Book — Cap 1-4 (1.5h)
  • Tokio tutorial: spawn, tasks, select! (1.5h)
Tarde (2h):
  • Ejercicio: echo server TCP con Tokio (1h)
  • Pin + Unpin + Futures (lectura selectiva) (1h)
```

### Día 2: Axum + Web
```
Mañana (3h):
  • Axum docs: router, handlers, extractors (1.5h)
  • Tower middleware stack + State (1.5h)
Tarde (2h):
  • Ejercicio: API REST con 3 endpoints + middleware auth simple (2h)
```

### Día 3: Traits + Error handling + Serde
```
Mañana (3h):
  • Rust for Rustaceans Cap 2-3 (tipos + traits) (1.5h)
  • anyhow + thiserror patterns (1.5h)
Tarde (2h):
  • serde: custom serialize/deserialize, JSON, MessagePack (1h)
  • Ejercicio: MemoryStore trait con 2 implementaciones (1h)
```

### Día 4: SQLite Internals
```
Mañana (3h):
  • SQLite architecture: B-tree, pager, VFS (1h)
  • WAL mode: cómo funciona, cuándo usarlo (1h)
  • FTS5: tokenizers, queries, ranking BM25 (1h)
Tarde (2h):
  • rusqlite bundled: compile features, Connection, statements (1h)
  • sqlite-vec: vector index, IVF-PQ (1h)
```

### Día 5: Postgres Internals
```
Mañana (3h):
  • Postgres architecture: processes, shared buffers, WAL (1h)
  • MVCC: transaction IDs, snapshots, visibility rules (1.5h)
  • Indexes: B-tree, GiST, GIN (0.5h)
Tarde (2h):
  • pgvector: HNSW, IVFFlat, cosine ops (1h)
  • tsvector + tsquery + RLS (1h)
```

### Día 6: SQL con sqlx + rusqlite
```
Mañana (3h):
  • sqlx: compile-time queries, migrations, pooling (1.5h)
  • sqlx + Postgres: execute, query_as, transactions (1.5h)
Tarde (2h):
  • Construir schema NexusMind en ambos motores (1h)
  • Benchmarks: sqlite vs postgres para caso de uso (1h)
```

---

## Semana 2 — Core (Auth + Crypto + MCP)

**Objetivo**: Implementar auth system, audit trail criptográfico, y MCP server.

### Día 1: OIDC + OAuth2
```
Mañana (3h):
  • OAuth2 roles + Authorization Code + PKCE (1.5h)
  • OIDC: ID Token, UserInfo, scopes (1.5h)
Tarde (2h):
  • JWT: structure, signing, validation en Rust (jsonwebtoken crate) (1h)
  • JWKS rotation + token refresh (1h)
```

### Día 2: SAML + SCIM + BYO IdP
```
Mañana (3h):
  • SAML2: SP-initiated SSO, assertions, metadata (1.5h)
  • SCIM 2.0: Users, Groups, provisioning (1.5h)
Tarde (2h):
  • saml-rs crate: implementar SP en Rust (1h)
  • Integración genérica con IdP externo (Okta, AzureAD) (1h)
```

### Día 3: RBAC + ABAC híbrido
```
Mañana (3h):
  • RBAC: roles, jerarquías, conflict resolution (1h)
  • ABAC: attributes, policies, evaluation engine (1.5h)
  • Hybrid model + role inheritance (0.5h)
Tarde (2h):
  • Casbin-rs: implementar policy engine (1h)
  • Ejercicio: 3 políticas de ejemplo para NexusMind (1h)
```

### Día 4: Zero Trust + Device Fingerprinting
```
Mañana (3h):
  • NIST SP 800-207: Zero Trust Architecture (lectura dirigida) (1.5h)
  • Session binding: user + device + tool (1h)
  • WebAuthn/passkeys: cómo funciona (0.5h)
Tarde (2h):
  • webauthn-rs crate (1h)
  • Device fingerprinting strategy para NexusMind (1h)
```

### Día 5: Criptografía Aplicada
```
Mañana (3h):
  • Merkle trees: construction, inclusion proof, batch verification (1.5h)
  • Ed25519: signing, verification, key management (1.5h)
Tarde (2h):
  • ed25519-dalek + sha2 en Rust (1h)
  • Implementar audit trail con Merkle chain (1h)
```

### Día 6: MCP Protocol
```
Mañana (3h):
  • MCP spec COMPLETO: host, client, server, transport (2h)
  • Resources, Tools, Prompts, Sampling (1h)
Tarde (2h):
  • Construir MCP server en Rust con axum/stdio (1.5h)
  • Testear con Claude Desktop (0.5h)
```

---

## Semana 3 — Avanzado (Vectors + Distributed + Sync)

**Objetivo**: Búsqueda semántica, CRDTs para sync, y el sync engine completo.

### Día 1: Embedding Models + ONNX
```
Mañana (3h):
  • Sentence embeddings: SBERT, all-MiniLM-L6-v2 (1h)
  • ONNX: modelo, input/output shapes, quantization (1h)
  • Candle: cargar modelo, inferencia, tensor ops (1h)
Tarde (2h):
  • Implementar embedding pipeline en Rust con Candle (1.5h)
  • Benchmark: latency vs batch size (0.5h)
```

### Día 2: Vector Search + Índices
```
Mañana (3h):
  • HNSW: algorithm, construction, search (1.5h)
  • IVF-PQ: product quantization, inverted files (1h)
  • Cosine similarity vs dot product vs euclidean (0.5h)
Tarde (2h):
  • sqlite-vec: insert, search, index IVF-PQ (1h)
  • pgvector: HNSW index, vector_cosine_ops (1h)
```

### Día 3: Hybrid Search (FTS + Vectors)
```
Mañana (3h):
  • Score fusion: weighted sum, Reciprocal Rank Fusion (1.5h)
  • BM25 + cosine hybrid pipeline (1h)
  • Re-ranking con cross-encoders (0.5h)
Tarde (2h):
  • Implementar hybrid search: FTS5 JOIN + sqlite-vec (1h)
  • Implementar hybrid search: tsvector + pgvector (1h)
```

### Día 4: Sistemas Distribuidos — Teoría
```
Mañana (3h):
  • CAP theorem + PACELC (1h)
  • Consistency models: strong, eventual, causal (1h)
  • Vector clocks + Lamport timestamps (1h)
Tarde (2h):
  • Diseñar consistency model de NexusMind (1h)
  • Leer DDIA Cap 5-6 (replication) — dirigido (1h)
```

### Día 5: CRDTs + Conflict Resolution
```
Mañana (3h):
  • CRDTs: state-based vs operation-based, merge rules (1.5h)
  • LWW Register, RGA (list), Map CRDT (1h)
  • Operational Transform vs CRDTs (0.5h)
Tarde (2h):
  • Leer paper de Kleppmann "Local-First Software" (1h)
  • Diseñar conflict resolution para NexusMind (1h)
```

### Día 6: Sync Engine
```
Mañana (3h):
  • SQLite → Postgres sync: strategies (changeset, WAL log, trigger-based) (1.5h)
  • Sync queue: retry, backoff, idempotency (0.5h)
  • Offline-first: pending writes, merge on reconnect (1h)
Tarde (2h):
  • Implementar sync queue + retry logic en Rust (1h)
  • Integrar con MemoryStore trait (1h)
```

---

## Semana 4 — Empresa (Security + DevOps + Enterprise)

**Objetivo**: Hacer NexusMind enterprise-ready.

### Día 1: Enterprise Security (SOC2 + GDPR)
```
Mañana (3h):
  • SOC2: trust services criteria (security, availability, confidentiality) (1.5h)
  • GDPR: data subject rights, consent, DPAs, data residency (1.5h)
Tarde (2h):
  • Compliance checklist para NexusMind (1h)
  • Encryption at rest/transit strategy (1h)
```

### Día 2: Key Management + Audit
```
Mañana (3h):
  • Key management: HSM, KMS, rotation policies (1.5h)
  • Audit logging: immutability, retention, tamper-proof (1.5h)
Tarde (2h):
  • OWASP Top 10 review (aplicado a NexusMind) (1h)
  • Threat modeling: session hijacking, data leak, privilege escalation (1h)
```

### Día 3: DevOps — CI/CD + Docker + ARM
```
Mañana (3h):
  • GitHub Actions: caching, release workflow, matrix builds (1.5h)
  • Docker multi-stage + distroless images (1h)
  • cross-rs: ARM64 cross-compilation para Rust (0.5h)
Tarde (2h):
  • Configurar CI/CD de NexusMind (1h)
  • Build ARM64 y testear (1h)
```

### Día 4: Observabilidad + Supabase
```
Mañana (3h):
  • OpenTelemetry: tracing, metrics, logging — en Rust (1.5h)
  • tracing crate: spans, events, subscribers (1h)
  • Supabase: setup, RLS policies, realtime subscriptions (0.5h)
Tarde (2h):
  • Implementar tracing + health checks en Axum (1h)
  • Configurar Supabase + schema initial (1h)
```

### Día 5: Componentes Clave de NexusMind
```
Mañana (3h):
  • MemoryStore trait: implementación final con SQLite backend (1h)
  • Policy Engine: evaluar ABAC policies en Rust (1h)
  • Audit logging: Merkle chain + Ed25519 signing (1h)
Tarde (2h):
  • Sync Engine: integración SQLite ↔ Postgres (1h)
  • MCP Server: expose tools + resources (1h)
```

### Día 6: Integración + Marketing Técnico
```
Mañana (3h):
  • Open source license + governance model (1h)
  • Documentation: API docs + quickstart + examples (1h)
  • Developer personality + competitive positioning (1h)
Tarde (2h):
  • Repasar ddia + todos los conceptos que quedaron sueltos (1h)
  • Plan de los próximos 30 días de código (1h)
```

---

## 📊 Horas Reales

| Semana | Horas | Temas cubiertos |
|--------|-------|----------------|
| 1 | 30h | Rust async, Axum, traits, SQLite, Postgres |
| 2 | 30h | OIDC, SAML, RBAC/ABAC, Zero Trust, Crypto, MCP |
| 3 | 30h | Embeddings, vector search, CRDTs, Sync Engine |
| 4 | 30h | SOC2, GDPR, DevOps, CI/CD, componentes finales |
| **Total** | **120h** | **12 temas completos** |

---

# 📚 Biblioteca Esencial (Lectura Obligatoria)

> Los recursos que DEBES leer/ver antes de escribir la primera línea de código.
> En el plan acelerado: leer las partes relevantes de cada libro, no el libro completo.

| Recurso | Tipo | Lectura dirigida | Prioridad |
|---------|------|-----------------|-----------|
| *Designing Data-Intensive Applications* — Kleppmann | Libro | Caps 5-9 (replication, partitioning, transactions, consistency) — ~8h | 🔴 |
| *Programming Rust (2nd Ed)* — Blandy | Libro | Caps 7-11, 19-21 (async, traits, error handling) — ~10h selectivo | 🔴 |
| MCP Specification (spec.modelcontextprotocol.io) | Docs | Completo — ~4h | 🔴 |
| SQLite Documentation (sqlite.org/docs) | Docs | WAL, FTS5, locking — ~4h | 🔴 |
| Postgres Internals (interdb.jp/pg) | Web | MVCC, indexes, RLS — ~4h | 🔴 |
| Rust Async Book | Web | Completo — ~4h | 🔴 |
| *Local-First Software* — Kleppmann et al. | Paper | [Paper link](https://martin.kleppmann.com/papers/local-first.pdf) — 1h | 🔴 |
| *NIST SP 800-207: Zero Trust* | Paper | [NIST](https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-207.pdf) — ejecutivo + sections 3-4 — 2h | 🔴 |
| *OAuth 2.0 Simplified* — Parecki | Libro | Completo — ~4h | 🟡 |
| *Rust for Rustaceans* — Gjengset | Libro | Caps 2-3 (types, traits) + Cap 7 (async) — ~6h selectivo | 🟡 |
| *CRDT paper* — Shapiro et al. | Paper | [Paper link](https://hal.inria.fr/inria-00609399v1/document) — 1h | 🟡 |

---

# 🔧 Stack Tecnológico de NexusMind (Resumen)

```
┌─────────────────────────────────────────┐
│  LENGUAJE                               │
│  Rust (edition 2024)                    │
│  • Async: Tokio                         │
│  • HTTP: Axum + Tower                   │
│  • DB drivers: rusqlite, sqlx           │
│  • Serialization: serde, serde_json     │
│  • ML: Candle (ONNX) / ort              │
│  • Crypto: ed25519-dalek, sha2          │
│  • Auth: webauthn-rs, casbin-rs         │
│  • Search: tantivy (optional)           │
│  • Logging: tracing                     │
├─────────────────────────────────────────┤
│  BASE DE DATOS                          │
│  • Local: SQLite (rusqlite bundled)     │
│  • Cloud: Postgres (sqlx)               │
│  • Vectors: sqlite-vec, pgvector        │
│  • FTS: FTS5 (SQLite), tsvector (PG)    │
│  • Sync: custom sync engine + CRDTs     │
├─────────────────────────────────────────┤
│  AUTH                                   │
│  • SSO: OIDC, SAML, OAuth2              │
│  • Políticas: ABAC + RBAC híbrido       │
│  • Device fingerprinting + Zero Trust   │
│  • Passkeys: WebAuthn                   │
├─────────────────────────────────────────┤
│  INFRA                                  │
│  • Cloud DB: Supabase (Postgres)        │
│  • Container: Docker + distroless       │
│  • CI/CD: GitHub Actions                │
│  • Observability: OpenTelemetry         │
│  • Cross-compile: cross-rs para ARM     │
└─────────────────────────────────────────┘
```

---

> **Mantén este archivo actualizado.** A medida que aprendas, marca checkboxes y añade notas. Es tu mapa de ruta personal para construir NexusMind.
