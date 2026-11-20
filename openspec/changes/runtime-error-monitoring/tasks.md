# Tasks — Runtime Error Monitoring

## Backend
- [x] Migration v103: `monitor_sources`, `monitor_groups`
- [x] Parse Cloud Run, GKE and Cloud Functions `LogEntry` (json, text, pino, Error Reporting shapes) and open incidents
- [x] Fingerprint (message normalization, top in-app frame) and throttle decision
- [x] Google OIDC verification (signature, iss, aud, email, expiry, tamper tests)
- [x] Public push endpoint and admin CRUD, test, groups endpoints
- [x] Slack alert (escaped) and GitHub issue (fenced, marker, regression) through org connectors only
- [x] Redaction before storage

## Admin
- [x] API client and types
- [x] Monitoring page: connect, setup commands, test, errors table with resolve/mute/reopen
- [x] Route `/monitoring` and sidebar entry

## Remaining
- [ ] CI green (full backend suite and clippy run in GitHub)
- [ ] Pilot on one Kasymir GCP project: run the setup commands, trigger an error, check Slack and the issue
- [ ] Optional: Cloud Monitoring alert-policy templates (restarts, error rate)
