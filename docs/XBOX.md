# Xbox One / Series (UWP, Dev Mode)

Super Smash Bros. 3DS Recompiled can run on an Xbox One or Xbox Series console in **Dev Mode** as a UWP app. The USA and Japan releases are **two separate apps**. Each one has its own package, tile, save data and folder, so you can install both side by side.

| App | Package name | Plays | Data folder on the Xbox |
|---|---|---|---|
| Super Smash Bros. 3DS Recompiled (USA) | `SmashBros3DSRecompiled.USA` | `00040000000EDF00` | `D:\DevelopmentFiles\SmashBros3DS-USA` |
| Super Smash Bros. 3DS Recompiled (Japan) | `SmashBros3DSRecompiled.JPN` | `00040000000B8B00` | `D:\DevelopmentFiles\SmashBros3DS-JPN` |

> **Status:** experimental and not yet tested on console hardware. The app builds and type-checks for the `x86_64-uwp-windows-msvc` target, and its input, layout, file-search and region logic is unit-tested. Xbox Dev Mode has no Vulkan, so the 3D is drawn by the CPU (software renderer). Unless you link in recompiled code (see below), the game's code is interpreted too. Expect it to run slower than full speed, especially on an Xbox One.

## 1. Put the console in Dev Mode

1. Register a Microsoft Partner Center developer account. This is a one-time paid registration.
2. Install the **Xbox Dev Mode** app from the Store on the console and follow its steps.
3. In **Dev Home**, turn on **Remote Access** (Device Portal) and note the address it shows, for example `https://192.168.1.50:11443`.

## 2. Install the app

Get `smash-3ds-xbox-usa-x64.msix` and/or `smash-3ds-xbox-jpn-x64.msix`, plus `Microsoft.VCLibs.x64.14.00.appx`. They come from the **Xbox (UWP) packages** GitHub Actions run, or you can build them yourself (see below).

1. Open the Device Portal address in a browser on your PC.
2. Under **Home → My games & apps**, choose **Add**, then pick the `.msix`.
3. On the next step, add `Microsoft.VCLibs.x64.14.00.appx` as a dependency, then start the install.
4. In Dev Home, select the installed app, then **View details**, and set **App type** to **Game**. This gives it the console's full game resources (CPU, GPU and memory) instead of the smaller app allowance.

## 3. Copy your game in

The default folder is `D:\DevelopmentFiles\SmashBros3DS-<region>`, and the app creates it on first launch.

1. In Device Portal, open **File explorer** and go to **DevelopmentFiles**.
2. Open (or create) `SmashBros3DS-USA` or `SmashBros3DS-JPN`.
3. Upload your own **decrypted** dump (`.3ds`, `.cci`, `.cxi` or `.cia`). Matching update and DLC `.cia` files go in the same folder; the newest update is used automatically.

The app also looks in `D:\DevelopmentFiles` itself, so if you drop both regions' dumps there, each app picks its own. If the console doesn't let the app use `D:\DevelopmentFiles`, it says so on screen and keeps its files in its own LocalState folder.

Saves (`user\`), the log (`smash-3ds.log`) and everything else the app writes are stored in that same folder, so you can back them up from Device Portal.

## Controls

The face buttons follow their position, not their letters, the way a Nintendo controller is laid out.

| Xbox controller | 3DS |
|---|---|
| A / B / X / Y | B / A / Y / X |
| LB / RB | L / R |
| LT / RT | ZL / ZR |
| Left stick, D-pad | Circle Pad, D-pad |
| Right stick | C-Stick |
| Menu / View | START / SELECT |
| Right stick click | Touch cursor on/off: the right stick moves it, RT touches |

Shortcuts (hold **View**, then press):

- **View + LB**: cycle the screen layout (large top screen, side by side, stacked, top screen only).
- **View + RB**: show/hide the frame rate.
- **View + Menu**: pause/resume.

The Xbox B button's system "back" action is captured, so pressing B won't close the game.

## Building the packages yourself

On Windows, you need:

- Visual Studio 2022 with **Desktop development with C++** and **Universal Windows Platform development**.
- The Windows 10/11 SDK.
- Rust, plus the nightly toolchain with the standard library source: `rustup toolchain install nightly --component rust-src`. `x86_64-uwp-windows-msvc` is a tier-3 target, so the standard library is built from source.

From a **Developer PowerShell for VS** (x64) prompt. The script links the app against Visual Studio's app ("store") CRT itself:

```powershell
./packaging/uwp/build-msix.ps1 -Region USA
./packaging/uwp/build-msix.ps1 -Region JPN
```

The packages, a self-signed `.cer` and the VCLibs dependency are written to `dist/uwp`. To build one region by hand: `cargo +nightly build -Z build-std --release --target x86_64-uwp-windows-msvc -p zakuro-uwp` for USA, or the same with `--no-default-features --features jpn` for Japan.

### Your own tile picture

The packages use the project's own icon. To put a picture of your choice, such as your game's cover, on the app's tiles and splash screen, give it to the build. It is added to the package only and never stored in the repository:

- **GitHub Actions:** in the repository's **Settings → Secrets and variables → Actions → Variables**, add `XBOX_ICON_URL` with a direct https link to the PNG or JPEG, then push or re-run the workflow. Once the workflow is on `main`, **Run workflow** also has an `icon_url` field.
- **Locally:** `./packaging/uwp/build-msix.ps1 -Region USA -Icon C:\pictures\cover.png`.

Packages built with someone else's artwork are for your own console; don't share them.

### Faster: linking recompiled code

An Xbox app can't run a C compiler or load a code library from a loose file, so the desktop **Recompile** button doesn't exist here. Instead, the recompiled code is linked into the app when you build it on your PC:

1. Use [3dsrecomp](https://github.com/fearkov/3dsrecomp) to port **your own** dump: `3dsrecomp port smash-usa.3ds C:\ports\smash-usa`. Use `CC=clang` and `AR=llvm-ar` with the MSVC Rust toolchain, as its docs describe.
2. Build the package with that `code` folder:

   ```powershell
   ./packaging/uwp/build-msix.ps1 -Region USA -RecompDir C:\ports\smash-usa\code
   ```

The resulting package contains code generated from your copy of the game. **Keep it for your own console and don't share it.** The app's on-screen notice at startup says whether it is running recompiled code or the interpreter.

Optional: `--features shader-jit` compiles the 3DS vertex shaders to native code on the CPU. The package declares the `codeGeneration` capability it needs, but this is off by default until it has been tried on a console.
