# Automation Operations Guide

## Overview

Autonomous Loop Engineering runs managed, bounded Claude Code workers with full provenance and policy enforcement.

## Managed Execution Profiles

1. `read-only`: Prohibits repository writes, PR publication, and deployment handoffs.
2. `implementation`: Permits creation of policy-approved PRs; denies direct merges and deployments.
3. `qa-deploy`: Invokes approved QA deployment handoff preserving human validation.

## Operator Controls & Emergency Stop

- **User-Applied Database Migration**: Ensure migration `v57` is applied prior to enabling worker execution.
- **Kill-Switch Execution**: Incrementing the organization policy generation invalidates all existing signed leases, prevents any pending callback writes, and revokes GitHub App credentials.
- **Audit & Evidence**: Every worker attempt records immutable receipts in `automation_receipts`.
