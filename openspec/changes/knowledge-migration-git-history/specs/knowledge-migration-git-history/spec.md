# Delta for Knowledge Migration — Git History Connector

## ADDED Requirements

### Requirement: History Is Read From The Local Repository

The connector MUST read commit history from a local checkout, including each commit's subject, body, author, date and touched files. It MUST work with no network access and no forge credentials.

#### Scenario: A repository with no remote still scans

- GIVEN a local repository with commits and no configured remote
- WHEN the connector scans it
- THEN it produces units from the commit history

#### Scenario: A path that is not a repository is refused clearly

- GIVEN a directory that is not a git repository
- WHEN the connector scans it
- THEN it reports that the path is not a repository
- AND it does not fail the enclosing run with an opaque error

### Requirement: Noise Is Filtered Before Any Model Is Called

The connector MUST discard mechanical commits by deterministic rule before any classification happens, and MUST report how many were discarded and why.

A commit is mechanical when its subject marks it as routine maintenance, when it is a merge with no explanatory body, when its author is a bot, or when its message carries no substance beyond the subject.

#### Scenario: Routine maintenance is discarded without a model

- GIVEN commits whose subjects mark them as chores, formatting, dependency bumps or work in progress
- WHEN the connector scans
- THEN none of them produces a unit
- AND no classification is performed for them

#### Scenario: A bot's commits are discarded

- GIVEN commits authored by an automation account
- WHEN the connector scans
- THEN they produce no units

#### Scenario: A merge with an explanation survives

- GIVEN a merge commit whose body explains what was decided
- WHEN the connector scans
- THEN it produces a unit

#### Scenario: The filter reports what it removed

- GIVEN a history containing both substantial and mechanical commits
- WHEN the connector scans
- THEN the report states how many commits were examined and how many were filtered
- AND the filtered ones are accounted for by reason

### Requirement: Identity Is The Commit

A unit's identity MUST be derived from the repository and the commit hash, which is immutable by construction. Re-scanning the same history MUST produce the same identities.

#### Scenario: Rescanning is idempotent

- GIVEN a repository scanned once
- WHEN it is scanned again with no new commits
- THEN every identity matches the previous scan

#### Scenario: Identity carries no absolute path

- GIVEN a repository at an absolute path on the operator's machine
- WHEN units are produced
- THEN no identity contains that path

### Requirement: Scanning Is Incremental

The connector MUST support scanning only the history after a given point, so a second run over a long-lived repository processes what is new rather than everything.

#### Scenario: Only new commits are scanned

- GIVEN a repository scanned up to a known commit
- WHEN it is scanned again starting after that commit
- THEN only the commits made since then produce units

### Requirement: The Unit Of Decision Is The Change, Not The Commit

Where a group of commits was merged as one unit of work, the connector MUST propose one candidate for the group rather than one per commit.

#### Scenario: A merged group proposes one candidate

- GIVEN a merge commit that names the change it merged, and the commits it brought in
- WHEN the connector scans
- THEN one unit represents the group
- AND the individual commits of that group do not each produce their own unit

### Requirement: Commit Shape Proposes The Destination

The connector MUST propose a destination from the shape of the commit message: a fix explaining a cause proposes a bug-fix memory, a change describing a choice proposes a decision memory, and anything else substantial proposes an architecture memory.

#### Scenario: A fix with a cause proposes a bugfix

- GIVEN a commit whose subject marks it as a fix and whose body explains the cause
- WHEN a candidate is produced
- THEN it proposes a memory of the bug-fix kind

#### Scenario: A revert is proposed with its context

- GIVEN a commit that reverts earlier work and explains why
- WHEN a candidate is produced
- THEN the candidate records that it is a reversal

### Requirement: Age Is Visible To The Reviewer

Every candidate MUST carry the date of the work it came from, so a reviewer can weigh whether a decision from two years ago still holds.

#### Scenario: The candidate shows when the work happened

- GIVEN a commit from a past date
- WHEN a candidate is produced
- THEN the candidate carries that date

### Requirement: Secrets In Commit Messages Do Not Reach Staging

Commit messages containing credential-shaped material MUST be redacted before submission, exactly as other sources are.

#### Scenario: A credential in a commit message is redacted

- GIVEN a commit whose body contains a credential-shaped token
- WHEN a candidate is produced
- THEN the token does not appear in the submitted content

### Requirement: Volume Is Estimable Before It Is Spent

The connector MUST report, without classifying anything, how many commits were examined, how many survived the filter, and an estimate of the tokens a full pass would consume.

#### Scenario: A dry run reports the survivors

- GIVEN a repository with a long history
- WHEN the connector runs in dry-run mode
- THEN it reports commits examined, units surviving the filter and estimated tokens
- AND no classification is performed
