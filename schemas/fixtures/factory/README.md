# Factory contract conformance fixtures

`cases.json` is the cross-language contract for the software factory's wire formats (`schemas/factory/*-v1.schema.json`).

Every fixture must be accepted or rejected **identically** by:

- the JSON Schema;
- every consumer's typed parser. Today that is the Rust types in `apps/backend/src/factory/contracts.rs`, checked by `apps/backend/tests/factory_contracts.rs`.

`cases.json` is generated from this tree: files under `valid/` must pass, files under `invalid/` must fail. After adding a fixture, regenerate it:

```sh
python3 - <<'PY'
import json, os
cases = []
for c in sorted(os.listdir('.')):
    if not os.path.isdir(c): continue
    for v in ('valid', 'invalid'):
        d = f'{c}/v1/{v}'
        for f in sorted(os.listdir(d)):
            cases.append({"contract": c, "fixture": f"{d}/{f}", "valid": v == 'valid'})
json.dump({"cases": cases}, open('cases.json', 'w'), indent=2)
PY
```

Changing an existing expectation means deciding whether the schema version must change. Consumers must never silently reinterpret a published fixture.
