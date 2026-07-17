# Pinned Global Goal

This repository implements the LinguaMesh global specification maintained by the central `linguamesh-project` repository.

Central repository: `linguamesh-project`

Authoritative local specification: [`../linguamesh-project/PROJECT_GOAL.md`](../linguamesh-project/PROJECT_GOAL.md)

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Verify a sibling checkout with:

```sh
sha256sum ../linguamesh-project/PROJECT_GOAL.md
```

On hosts without GNU coreutils, use:

```sh
shasum -a 256 ../linguamesh-project/PROJECT_GOAL.md
```

The emitted digest must match the value above. A digest change requires reviewing the complete new specification, updating compatibility records in the central repository, and deliberately updating this pin. This file is a revision reference, not a claim that the native client is implemented.
