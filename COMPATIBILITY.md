# Compatibility

This repository is configured specifically for **Super Smash Bros. for Nintendo 3DS**. It accepts the regional base-game title IDs below and hides unrelated titles from the game library.

| Region | Base-game title ID | Update title ID | DLC title ID | Library support |
|---|---|---|---|---|
| Japan | `00040000000B8B00` | `0004000E000B8B00` | `0004008C000B8B00` | Accepted |
| USA | `00040000000EDF00` | `0004000E000EDF00` | `0004008C000EDF00` | Accepted |
| Europe / Australia | `00040000000EE000` | `0004000E000EE000` | `0004008C000EE000` | Accepted |

Upstream Zakuro's compatibility list reports the USA base game as playable with ahead-of-time recompiled code. That is an upstream compatibility result, not a claim that this game-specific fork has passed independent end-to-end tests.

## Updates

The update title family uses the `0004000E` namespace and preserves the regional suffix of the base title. Place a decrypted update CIA beside its matching base dump. The library selects the highest matching update version it finds.

## Downloadable content

The DLC title family uses the `0004008C` namespace and preserves the same regional suffix:

- Japan: `0004008C000B8B00`
- USA: `0004008C000EDF00`
- Europe / Australia: `0004008C000EE000`

Place decrypted DLC CIA files beside the matching base-game dump. The library recognizes them as DLC and associates them with that game. Tests cover all three regional DLC title IDs and verify the add-on association path. This does not guarantee every DLC item has been validated in actual gameplay.

## Game files and recompilation

Accepted game file extensions are `.3ds`, `.cci`, `.cxi` and `.cia`. A legally obtained, decrypted dump is required. Encrypted or damaged data may be displayed with an explanatory warning but cannot be recompiled by this build.

Generated native code is created locally and is not committed to this repository, CI artifacts or GitHub Releases. Do not treat a title ID being accepted by the library as proof that its content has been tested.

When reporting a compatibility issue, include the game region, title ID, project version, operating system, whether an update or DLC was present, and the relevant runtime log.
