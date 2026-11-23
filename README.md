# Super Smash Bros. 3DS Recompiled

An unofficial, game-specific desktop build for **Super Smash Bros. for Nintendo 3DS**.

This project is based on [Zakuro](https://github.com/fearkov/zakuro), a high-level-emulation Nintendo 3DS runtime, and [3dsrecomp](https://github.com/fearkov/3dsrecomp), its ahead-of-time ARM code recompiler. This fork is configured to accept the three regional releases of Super Smash Bros. for Nintendo 3DS and their matching updates and downloadable content.

**Project version: 1.0.1**

> **Compatibility:** Upstream Zakuro's compatibility list reports the USA release as playable with AOT-recompiled code. This standalone fork has not yet been independently validated end-to-end on every platform, region, update or DLC item. See [COMPATIBILITY.md](COMPATIBILITY.md).

## Supported game dumps

| Region | Base game title ID | Update title ID | DLC title ID |
|---|---|---|---|
| Japan | `00040000000B8B00` | `0004000E000B8B00` | `0004008C000B8B00` |
| USA | `00040000000EDF00` | `0004000E000EDF00` | `0004008C000EDF00` |
| Europe / Australia | `00040000000EE000` | `0004000E000EE000` | `0004008C000EE000` |

Accepted base-game file formats are `.3ds`, `.cci`, `.cxi` and `.cia`. A legally obtained, decrypted dump is required. Encrypted files cannot be recompiled by this build.

### Updates and DLC

Place a matching decrypted update CIA and any matching DLC CIA beside the base-game dump in the same game-library folder. The library pairs add-ons with the base game by their regional title-ID suffix, keeps the newest matching update, and attaches matching DLC to the title. The runtime's generic add-on handling is covered by regional title-ID tests.

DLC for Super Smash Bros. for Nintendo 3DS includes additional fighters and Mii Fighter costumes. This project does not distribute DLC files, unlock data, tickets, keys, or any other game content. Each update and DLC combination still needs real-game validation before it can be described as fully working.

## Version 1.0.1

- Fixes startup when no update CIA is installed by falling back to the base game's RomFS for update-RomFS requests.
- Adds diagnostic logging for failed filesystem opens.

## Version 1.0.0

- Establishes a dedicated Super Smash Bros. for Nintendo 3DS desktop build.
- Restricts the library and direct-launch checks to Japan, USA and Europe/Australia base-game title IDs.
- Recognizes the corresponding regional update and DLC title families.
- Uses dedicated application naming, settings, save-data and log directories.
- Builds on Windows, macOS Apple silicon, macOS Intel and Linux through GitHub Actions.
- Keeps generated title-specific native code and game-derived content on the user's own computer.

Version 1.0.0 is the initial game-specific project baseline. It does not claim independent gameplay verification across all regions or operating systems.

## How to play

1. Download and extract the package for your operating system from [GitHub Releases](https://github.com/nvd123-YT/SuperSmashBros3DSRECOMP/releases).
2. Start **Super Smash Bros. 3DS Recompiled**.
3. Choose the folder containing your own decrypted game dump. Keep matching update and DLC CIA files in that same folder.
4. Select the game and press **Recompile**. The ARM code is translated into native code locally by 3dsrecomp. A C compiler such as Clang or GCC is required; the application can offer to download Zig when no supported compiler is available.
5. Start the game. Recompiled code is used where available, with interpreter fallback for code that has not been translated.

The generated title-specific native code, saves, settings and logs stay on your computer. They are not uploaded by this project.

You can also prepare the local recompilation from a terminal:

```sh
smash-3ds --prepare --game /path/to/your/decrypted-game.3ds
```

Replace the example path with the path to your own dump.

## Runtime features

This game-specific build inherits the desktop features implemented by Zakuro:

- Ahead-of-time ARM code recompilation through 3dsrecomp, with interpreter fallback.
- Keyboard and game-controller input with configurable mappings.
- Desktop graphics, 3DS screen layouts and scaling options.
- Audio settings and per-game configuration.
- Local save data, save states, match replay recording/playback, logs and optional mod/plugin support where implemented by the runtime.

Individual game features, online services, peripheral behavior, updates and DLC may require further compatibility work. Accepted title IDs are not a guarantee that every feature has been tested.

## Match replays

The pause menu's **Match replays** section can record and replay a complete run without command-line environment variables:

- **F6** starts a recording from a fresh game boot. Press F6 again, or choose **Stop and save recording**, to finish it.
- **F7** restarts the game and plays the most recent recording. Live keyboard/controller input is ignored during playback.
- **Open recordings folder** opens the per-game recording directory. The latest recording is stored as `replays/<TITLE_ID>/latest.replay` inside the application's data folder.

Recordings contain the input stream and original emulated clock, not game assets. Use the same game dump, update, DLC and settings when replaying for the most reliable result. Starting a recording deliberately restarts the game so playback can reproduce the entire run from its first frame.

## Downloads

The release workflow is configured to produce:

- **Windows x64:** `smash-3ds-windows-x86_64.exe` and `smash-3ds-windows-x86_64.zip`.
- **macOS Apple silicon:** `smash-3ds-macos-arm64.dmg` and ZIP, containing `Super Smash Bros. 3DS Recompiled.app`.
- **macOS Intel:** `smash-3ds-macos-x86_64.dmg` and ZIP.
- **Linux x64:** `smash-3ds-linux-x86_64.AppImage` and `smash-3ds-linux-x86_64.tar.gz`.

The macOS app is ad-hoc signed but not notarized. On first launch, macOS may block it; allow it through **System Settings → Privacy & Security → Open Anyway**. The Linux AppImage requires the host system's ALSA sound library.

## Build from source

The project uses a Rust workspace and requires Rust 1.95 or newer. Game-code recompilation also needs an available C compiler or Zig. Build and test the workspace with:

```sh
cargo build --release --locked -p zakuro
cargo test --release --workspace
```

The desktop binary is written to `target/release/smash-3ds` (or `.exe` on Windows).

On macOS, package a functional application bundle and create both a ZIP and a compressed DMG:

```sh
sh packaging/macos-app.sh target/release/smash-3ds 1.0.0 smash-3ds-macos-arm64.zip
```

The script bundles MoltenVK into the app and creates a matching `.dmg`.

## GitHub Actions

- **CI** builds and tests the Rust workspace on Windows, macOS and Linux.
- **macOS test build** packages the application and performs a smoke test.
- **Release** produces Windows, macOS and Linux packages. Push a version tag such as `v1.0.0` after the initial branch is ready; the release workflow publishes only after the platform builds succeed.
- Pushes to `main` run build/test jobs and keep temporary artifacts. They do not publish a GitHub Release.

The workspace and application bundle version are both set to `1.0.1`. No release tag is created automatically by this setup.

## RecompLauncher

The repository includes [recomplauncher.json](recomplauncher.json), a game-specific manifest for selecting the correct dump, preparing local recompilation, launching the executable, and locating settings, saves and logs.

## Legal and attribution

This is an unofficial, game-specific fork based on [fearkov/zakuro](https://github.com/fearkov/zakuro) and [fearkov/3dsrecomp](https://github.com/fearkov/3dsrecomp). Third-party notices are preserved in [LICENSE-MIT](LICENSE-MIT) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The project is distributed under GPL-3.0-or-later; see [LICENSE](LICENSE).

Super Smash Bros. for Nintendo 3DS and related marks are trademarks of Nintendo. This project is not affiliated with, endorsed by or sponsored by Nintendo. No game dump, decryption keys, generated title-specific C code, compiled game library or copyrighted game assets are distributed. Users must provide their own lawful game files and keep generated title-specific output on their own device.
