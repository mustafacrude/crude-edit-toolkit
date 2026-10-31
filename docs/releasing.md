# Releasing

How a new version gets to people. There is one public repo, `mustafacrude/crude-edit-toolkit`: it holds the code and the workflows, and its Releases page holds the installers and the update file (`latest.json`), so friends can download and the app can update without signing in to anything.

## One-time setup

1. **Create the repo** (public) and push the code to it.
2. **Back up the updater key.** It was generated at `%USERPROFILE%\.tauri\crude-toolkit-updater.key` (with `.key.pub` beside it). Copy it somewhere safe (a password manager). Once a release ships with its public key (it is in `src-tauri/tauri.conf.json`), the private key is the only thing that can sign updates for already-installed copies. If it is lost, those copies can never update and people must reinstall by hand.
3. **Add two secrets** to the repo (Settings → Secrets and variables → Actions):
   - `TAURI_SIGNING_PRIVATE_KEY`: the full contents of the `.key` file.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password. The key was made without one, so leave this empty or don't add it.

   The workflow publishes with its own built-in token, so there is no other token to make. Secrets are never exposed to forks or pull requests from other people.

## Each release

0. Optionally run **CI** by hand first (Actions tab → CI → Run workflow). It is not automatic, to save Actions minutes.
1. Change the version in **three places** and keep them identical: `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (then `cargo check` to refresh `Cargo.lock`).
2. If a bundled tool should move, edit `tools/manifest.json` (`url` and `sha256` together) and run `npm run tools` to test it. Then save that tool's licence text(s) for the new version into `licenses/tools/`, and run `npm run licenses` (also after any dependency change) to regenerate the licence list the installer ships. FFmpeg's source is attached to each release by the workflow, from the commit named in its manifest URL.
3. Commit, tag and push the tag:
   ```
   git tag v0.1.1
   git push origin main v0.1.1
   ```
4. The **Release** workflow builds the Windows installer (and, once it's switched on, the Mac one) and attaches it, with `latest.json`, to a **draft** release of this repo. The workflow fails early if the tag doesn't match the app's version.
5. Open the draft, check the files, write the notes (they show in the app's update card), and **publish**. Publishing is what makes `releases/latest/download/latest.json` point at the new version; until then nobody sees it.

## Things to know about the installer

- The product name uses a typographic apostrophe (`Crude’s Edit Toolkit`, U+2019), not a plain `'`. NSIS wraps the name in single quotes in its own scripts, so a plain apostrophe makes the installer build fail ("macro NSISCOMCALL requires 4 parameter(s), passed 8"). The window title keeps the plain one.
- That character is also in the installer's file name. On the first draft release, check that GitHub kept the asset names and that `latest.json` points at them (the workflow takes the URLs from what GitHub reports, so it should).
- The installer is NSIS only, installs per user into `%LOCALAPPDATA%`, and adds Start menu and desktop shortcuts, an Installed apps entry and the "Send to Crude's Toolkit" right-click entries. Uninstalling removes all of them. It adds nothing that runs at sign-in.
- Windows' toast notifications work because Tauri's installer stamps the shortcuts with the app identifier (`net.crudes.edittoolkit`).

## Testing the update path

The app can only be shown an update by a real published release, so it takes two: publish `0.1.0`, install it, then publish `0.1.1` and open the installed app. It should offer "Update available" within a day of its last check, or immediately from Settings → Updates and notifications → *Check for updates*.

## Mac

The Mac job is commented out in `.github/workflows/release.yml` until an Apple Silicon `ffmpeg` and `ffprobe` are pinned (the `aarch64-apple-darwin` entry for ffmpeg in `tools/manifest.json` is `null`; `fetch-tools` skips a tool with no pinned build, so the build would only fail at the very end). The app is ad-hoc signed (`"signingIdentity": "-"`) and not notarized, so macOS needs *Open Anyway* the first time. None of the Mac code paths (Finder's Open With, the `.dmg`, VideoToolbox) has been run on a Mac.
