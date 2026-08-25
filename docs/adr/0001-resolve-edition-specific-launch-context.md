# ADR 0001: Resolve an edition-specific Launch Context before side effects

- Status: Accepted
- Date: 2026-08-24

## Context

D2RHub supports CN and Global Diablo II: Resurrected installations. They can have
different Battle.net executables, game directories, save directories, product
codes, configuration keys, regions, locales, and authentication behavior.

Keeping those rules as independent CN-or-Global branches allowed an Account to
combine paths or conventions from different installations. Legacy migration
could also copy one Battle.net path into both editions. Some launch and
reinitialization flows terminated processes before proving that the target
Account had a complete, valid configuration.

## Decision

Every operation that reads settings, prepares authentication, launches
Battle.net, or launches the game resolves a typed, immutable LaunchContext
first.

The resolver:

- parses Game Region and authentication mode explicitly and rejects unknown
  values;
- maps Game Region to one Client Edition;
- selects all paths from exactly one Installation Profile, with no cross-edition
  fallback;
- validates absolute paths, Battle.net.exe, and D2R.exe according to the
  operation's capabilities;
- keeps Battle.net product/config conventions separate from Token auth/registry
  conventions;
- rejects duplicated CN and Global installation paths;
- permits legacy region inference only when exactly one Client Edition is
  configured.

Batch launch performs a no-side-effect preflight for every Account, then obtains
an exclusive host runtime lease before changing shared Battle.net files,
registry state, Agent processes, or game processes.

The persisted v3 configuration keeps six edition-specific fields: Battle.net,
game, and save paths for CN, plus the same three paths for Global. Legacy values
are migrated only when their edition can be inferred without ambiguity.

Battle.net and UnifiedAuth remain machine-wide resources. Operations therefore
use two explicit leases in one lock order:

1. acquire every affected Account Lifecycle Lease in sorted account-id order;
2. copy the global configuration and release its read lock;
3. resolve and validate the Account's Launch Context;
4. acquire the Host Runtime Lease for every game-launch batch because both authentication modes mutate machine-wide runtime state;
5. reload and resolve the Account under the host lease before side effects.

Token-only account metadata initialization does not acquire the Host Runtime Lease. Token game launch does acquire it because it writes the shared `Launch Options\\OSI` registry key and may replace the edition-specific saved-game `Settings.json` before spawning the game.

## Runtime snapshot transaction

New Battle.net snapshots use this canonical layout:

```text
accounts/{account_id}/runtime/
  Battle.net/
    Battle.net.config
  unified_auth.json
  snapshot.json
```

`snapshot.json` records the schema version, Client Edition, and creation time.
Restore validates the complete bundle and rejects a snapshot whose edition does
not match the resolved Launch Context.

Snapshot capture never updates loose files in place. It copies the Account to a
sibling temporary directory, builds and validates the runtime bundle there,
updates `account.json` there, cleans the shared host state, and finally swaps the
whole Account directory once. A `.bak` directory supports rollback and is
restored automatically if a prior swap was interrupted after moving the target.
The host Battle.net Roaming directory uses the same sibling `.tmp`/`.bak` swap,
so a failed copy cannot leave the machine-wide runtime half-restored. Account
`Settings.json` and its customization metadata are also committed through one
Account-directory transaction.

Existing loose `Battle.net/` plus `unified_auth.json` or strictly validated
`unified_auth.reg` snapshots remain a read-only compatibility path. The next
successful write-back migrates them to the canonical runtime bundle. A legacy
snapshot without provenance is accepted only when `Battle.net.config` contains
exactly one recognizable product key (`osic` for CN or `osi` for Global) and it
matches the Account's resolved Client Edition; ambiguous snapshots require
reinitialization. An edition change marks the Account uninitialized; the next
initialization starts from a clean shared runtime and never restores the previous
edition's snapshot.

## Consequences

- CN and Global Accounts can coexist without sharing installation paths or
  launch conventions.
- Invalid or incomplete paths fail before processes, files, or registry values
  are changed.
- Account deletion and metadata edits cannot race with initialization or launch
  write-back, and `save_meta` never recreates a deleted directory.
- A runtime snapshot, its provenance, and initialization metadata become visible
  together or not at all.
- Battle.net workflows are intentionally serialized because Blizzard's roaming
  directory and UnifiedAuth registry are host-wide rather than per installation.
- Token launches are serialized by the same Host Runtime Lease because their
  registry token and pre-launch saved-game settings are also machine-wide state.
- The first successful write-back of a legacy Account performs a recoverable
  on-disk migration and may temporarily require additional disk space.
