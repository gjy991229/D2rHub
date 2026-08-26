# D2RHub

D2RHub coordinates multiple local Diablo II: Resurrected accounts across distinct client editions while keeping each account's launch and settings state isolated.

## Language

**Account**:
A locally managed Diablo II: Resurrected identity with isolated authentication, settings, and launch state.
_Avoid_: Client, profile

**Game Region**:
The online region selected by an account: CN, Asia, Americas, or Europe. Asia, Americas, and Europe all use the Global Client Edition.
_Avoid_: Client version, installation

**Client Edition**:
One of the two locally installable game distributions: CN or Global. Each edition owns its game and save directories. Only the CN edition can additionally own the single configured Battle.net executable.
_Avoid_: Server, Game Region

**Installation Profile**:
The configured local paths for one Client Edition. Token authentication requires its game and save directories. Battle.net authentication is a CN-only compatibility mode and additionally requires the single CN Battle.net executable.
_Avoid_: Global path, shared path

**Launch Context**:
The complete, validated edition-specific paths and conventions resolved for one Account launch.
_Avoid_: Global config, launch params

**Runtime Snapshot**:
The versioned authentication bundle stored under an Account's `runtime/` directory. It contains the exact Battle.net directory, UnifiedAuth JSON, and a Client Edition manifest and is committed together with `account.json` through one Account-directory transaction.
_Avoid_: Token file, loose backup

**Account Lifecycle Lease**:
An exclusive lease for writes to one Account directory. It prevents initialization, launch write-back, delete, rename, settings, and metadata updates from racing. Its key is ASCII-case-normalized so UUID aliases cannot acquire different leases on Windows.
_Avoid_: Global account lock

**Host Runtime Lease**:
The exclusive lease for machine-wide Battle.net, Agent, roaming data, UnifiedAuth, Token launch registry, and saved-game Settings state. The lock order is always Account Lifecycle Lease before Host Runtime Lease. Every game launch acquires it; Token-only account metadata operations do not.
_Avoid_: Account lock
