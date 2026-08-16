# Delta for Knowledge Migration — Database Schema Connector

## ADDED Requirements

### Requirement: The Default Reads No Business Rows

With no explicit opt-in, the connector MUST read only catalog metadata — tables, columns, types, nullability, defaults, keys, constraints, indexes, views and comments — and MUST NOT read a single row of a business table.

#### Scenario: A default run touches no business data

- GIVEN a database holding business tables with rows
- WHEN the connector runs with default options
- THEN it produces units describing the schema
- AND no business row is read

#### Scenario: The default is reported, not assumed

- GIVEN a default run
- WHEN the report is produced
- THEN it states that no data was sampled

### Requirement: The Connection Must Be Read-Only

The connector MUST refuse to run against a role that can write. Refusal MUST name the problem.

#### Scenario: A writable role is refused

- GIVEN credentials for a role with write privileges
- WHEN the connector starts
- THEN it refuses to scan
- AND the refusal says the role must be read-only

#### Scenario: A read-only role proceeds

- GIVEN credentials for a role that can only read
- WHEN the connector starts
- THEN it scans normally

### Requirement: Credentials Never Appear In A Command Line Or A Record

The connection string MUST be supplied by environment variable or interactive prompt, never as a command-line argument, and MUST NOT be persisted on the run.

#### Scenario: A connection string passed as an argument is refused

- GIVEN a connection string supplied as a command-line argument
- WHEN the connector starts
- THEN it refuses and explains that the value would persist in shell history

#### Scenario: The run records the database, not the credentials

- GIVEN a scan against a database
- WHEN the run is recorded
- THEN it identifies the database without carrying the password or the full connection string

### Requirement: Knowledge Is Grouped By Area, Not By Table

The connector MUST propose one candidate per schema area rather than one per table. A schema of two hundred tables MUST NOT produce two hundred candidates.

#### Scenario: Related tables become one candidate

- GIVEN several tables belonging to one schema
- WHEN the connector scans
- THEN it proposes one candidate describing that area
- AND that candidate names the tables it covers

#### Scenario: A large schema stays reviewable

- GIVEN a database with many tables across a few schemas
- WHEN the connector scans
- THEN the number of candidates is proportional to the number of areas, not to the number of tables

### Requirement: Constraints Are Carried As Knowledge

A candidate MUST carry the rules the schema enforces — accepted values, foreign keys and their delete behaviour, uniqueness — because those are the business rules an agent otherwise discovers by breaking something.

#### Scenario: An enumerated column reports its accepted values

- GIVEN a column constrained to a set of values
- WHEN a candidate is produced for its area
- THEN the accepted values appear in the candidate

#### Scenario: A restricted foreign key reports its behaviour

- GIVEN a foreign key that restricts deletion
- WHEN a candidate is produced
- THEN the candidate states the relationship and that deletion is restricted

### Requirement: Sampling Data Requires Four Conditions Together

Reading business rows MUST require all of: an explicit table allowlist, a bounded row limit, redaction applied before the sample leaves the process, and an operator attestation recorded on the run. Missing any one MUST refuse the sample.

#### Scenario: Sampling without an allowlist is refused

- GIVEN sampling requested with no table allowlist
- WHEN the connector starts
- THEN it refuses and names the missing condition

#### Scenario: Sampling without an attestation is refused

- GIVEN sampling requested with an allowlist and a limit but no attestation
- WHEN the connector starts
- THEN it refuses and names the missing condition

#### Scenario: A table outside the allowlist is never sampled

- GIVEN sampling enabled for one table
- WHEN the connector scans a schema containing others
- THEN only the allowlisted table is sampled

#### Scenario: All four conditions together permit sampling

- GIVEN an allowlist, a limit, redaction enabled and an attestation
- WHEN the connector scans
- THEN the allowlisted table is sampled up to the limit
- AND the attestation is recorded on the run

### Requirement: Sampled Values Are Redacted Before They Leave The Process

Where rows are sampled, personally identifying values MUST be redacted locally before the sample becomes part of any candidate.

#### Scenario: Identifying values do not reach a candidate

- GIVEN a sampled row containing an email address and a credential-shaped token
- WHEN a candidate is produced
- THEN neither value appears in it
- AND the redaction is reported

### Requirement: Supabase Specifics Are Recognised

Where the database is a Supabase project, the connector MUST additionally describe its row-level security policies, because those are access rules and access rules are knowledge.

#### Scenario: Access policies are described

- GIVEN a table with row-level security policies
- WHEN the connector scans in Supabase mode
- THEN a candidate describes those policies

### Requirement: Cost And Coverage Are Reportable Before Spending

The connector MUST report, without classifying anything, how many areas and tables it found, whether sampling was enabled, and an estimate of the tokens a full pass would consume.

#### Scenario: A dry run reports coverage and states the sampling mode

- GIVEN a database
- WHEN the connector runs in dry-run mode
- THEN it reports areas, tables and estimated tokens
- AND it states whether any data would be sampled
- AND no classification is performed
