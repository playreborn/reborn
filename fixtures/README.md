# Fixtures

Recorded, **redacted** test data. Replay tests use these files so that no
test ever has to talk to Valve's live servers.

## Layout (planned)

```
fixtures/
├── cm/        # recorded CM messages per flow (auth, pics, social, ...)
├── formats/   # larger format samples (appinfo, depot manifests) once needed
└── raw/       # unredacted recordings, git-ignored, NEVER commit
```

Small samples that belong to one crate live next to its tests instead
(e.g. `crates/formats/tests/data/`), so the crate stays self-contained when
published.

## Rules

1. Recordings are made only with a **secondary test account**.
2. Before anything leaves `raw/`, it is redacted automatically: tokens,
   passwords, Steam IDs, account names, e-mail addresses, IP addresses and
   machine identifiers are replaced with stable placeholders.
3. Check the redacted file by hand before committing it.
4. Depot manifests (metadata) are fine. Game content (chunk data, game files)
   is never committed.
5. Every fixture notes where and when it was recorded (Reborn commit, date).

The recorder and redaction tool are part of milestone M0 (`reborn-testkit`).
