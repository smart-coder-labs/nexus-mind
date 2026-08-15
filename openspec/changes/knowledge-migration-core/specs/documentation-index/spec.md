# Delta for Documentation Index

## ADDED Requirements

### Requirement: Documentation Corpus Is Separate From Code

Documentation chunks MUST be stored in a corpus distinct from the code corpus. Indexing documentation MUST NOT change the results of code search.

#### Scenario: Code search results are unaffected

- GIVEN a project whose code is already indexed
- WHEN its documentation is indexed
- THEN a code search returns the same results and the same ranking as before
- AND no documentation chunk appears in code search results

#### Scenario: Documentation search returns documents

- GIVEN a project whose documentation is indexed
- WHEN a documentation search is issued
- THEN matching documentation chunks are returned with their file path and section
- AND no code chunk appears in the results

### Requirement: Documentation Chunking Preserves Structure

Documentation MUST be chunked by document structure, and each chunk MUST retain the path of the document and the heading hierarchy of the section it came from.

#### Scenario: A long document yields addressable sections

- GIVEN a document with several headed sections
- WHEN it is indexed
- THEN each section becomes a separately addressable chunk
- AND each chunk records its document path and heading path

### Requirement: Indexing Is Independent of Migration Approval

A document scanned by a connector MUST be indexed regardless of whether the candidates derived from it are approved, rejected, or still staged. Rejecting a candidate MUST NOT remove its source document from the index.

#### Scenario: Rejected candidate leaves the document indexed

- GIVEN a document was scanned and produced a candidate
- WHEN the candidate is rejected
- THEN the document remains searchable in the documentation index

#### Scenario: Indexing occurs without any candidate

- GIVEN a document that yields no candidate at all
- WHEN it is scanned
- THEN it is still indexed and searchable

### Requirement: Indexing State Is Observable and Reconcilable

The system MUST record whether a committed artifact and an indexed document have been vectorized. When vectorization is unavailable or fails, the artifact MUST still be persisted, the indexing state MUST reflect that it is not vectorized, and a reconciliation path MUST exist to vectorize it later.

#### Scenario: Commit succeeds when vectorization is unavailable

- GIVEN no embedding service is configured
- WHEN a candidate is committed
- THEN the destination record is created
- AND its indexing state records that it is not vectorized
- AND the commit is reported as successful

#### Scenario: Reconciliation vectorizes what was missed

- GIVEN artifacts exist that were persisted without vectors
- WHEN reconciliation runs with an embedding service available
- THEN those artifacts are vectorized
- AND their indexing state is updated

#### Scenario: Unvectorized content is reported

- GIVEN some documents or artifacts are not vectorized
- WHEN the indexing state is queried
- THEN the system reports how many are pending and why
