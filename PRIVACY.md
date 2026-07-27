# Privacy

LinguaMesh Linux has no telemetry or account. Translation content leaves the machine only when
the user selects a remote provider and invokes a translation. Loopback and local-model endpoints
can keep content local.

GTK secret fields are masked and cleared immediately after capture. Core receives credentials and
client-certificate identities only through one-shot host-secret responses; SQLite stores SecretRefs
and non-secret profile metadata, never secret values, source text, translated output, or private
keys. Session-only mode remains available when Secret Service persistence is unavailable. History,
translation memory, and document persistence follow explicit privacy policies, with Incognito
bypassing content persistence.

Provider privacy terms govern remote requests. Report privacy defects through `SECURITY.md` without
including credentials, private documents, or sensitive diagnostics.
