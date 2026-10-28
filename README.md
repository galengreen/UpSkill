# UpSkill

One library for every agent's skills, on every machine.

Claude Code, Codex, Amp, Cursor, Gemini CLI, OpenCode, Copilot CLI, Factory Droid and Pi each keep `SKILL.md` folders in their own places. UpSkill keeps **one versioned library** in `~/.upskill/library` and places each skill into the folders the right agents read. It tracks which skills actually get used, catches the ways skills rot, syncs across machines without git chores, and exposes all of it to agents over MCP.

## What it does

- **One library, smart placement.** Skills are symlinked into agent folders. Several folders are read by more than one agent (Amp reads `~/.claude/skills`, Cursor reads `~/.codex/skills`), so UpSkill picks the fewest folders that reach the agents you chose without leaking to others, and tells you when a leak can't be avoided.
- **Groups and targeting.** Target a skill (or a whole group) at specific agents and machines. If you set nothing, a skill goes everywhere. Dependencies follow automatically: if `full-stop` calls `full-send`, `full-send` goes wherever `full-stop` goes.
- **History for every change.** Every edit is a content-addressed version, whether it was made by you, by an agent through MCP, or outside the app (an agent editing through a symlink). You get diffs, one-click restore, and a **bloat alert** when agent edits have grown a skill since you last touched it.
- **Usage.** Read incrementally from Claude Code and Codex session logs (the first scan of ~1.7 GB took ~3 s). It shows invocations per skill and per agent, manual vs automatic use, and "gone dark" skills worth archiving.
- **Health.**
  - Checks against the agentskills.io spec.
  - Treats descriptions as triggers ("Use when…", trigger words in the first 250 chars).
  - Estimates the **context budget** per agent and shows which skills will lose their description first.
  - Detects **trigger collisions** between skills.
  - Runs security checks: pipe-to-shell, credential paths, prompt-injection phrasing, invisible Unicode, unpinned `npx`/`uvx`.
  - Finds broken symlinks and diverging copies of the same skill.
- **Adoption.** Finds every unmanaged skill already on disk. Adopting backs up the originals, keeps every differing copy in history and preserves which agents could see it.
- **Install from a repo.** `owner/repo` or a URL. The source is tracked, so upstream updates can be pulled later and your local version stays in history.
- **Sync without git chores.** Sync goes through any shared folder (Syncthing, Dropbox, iCloud, a NAS) or a private GitHub repo, with git kept out of sight. Each machine writes only its own manifest plus immutable objects, so the transport never sees a conflict. Concurrent edits to one skill merge deterministically: the newest wins and the other stays in history. Usage from every machine is combined.
- **MCP for agents.** 36 tools cover everything the app can do. A companion `upskill` skill teaches agents to use them and to leave **notes** when a skill misfires.
- **Tray / menu bar.** Shows status and has Sync now and Rescan. Closing the window keeps UpSkill running.

## Layout

```
crates/core   Rust core: library, store, placement, usage, lint, health, sync, JSON API
crates/cli    `upskill` binary: CLI, MCP server (stdio), dev HTTP bridge
src-tauri     Desktop shell: window, tray, file watcher, background worker
src           React UI (one `api` bridge command → core API)
```

## Develop

```sh
bun install
bun run sidecar        # builds the upskill CLI and stages it for Tauri (needed once)
bun run app            # tauri dev
cargo test -p upskill-core
```

To work on the UI in a browser against a sandbox, use the dev bridge:

```sh
UPSKILL_HOME_DIR=/tmp/demo/home UPSKILL_ROOT=/tmp/demo/root target/release/upskill serve 7777
bunx vite --port 1420
```

`UPSKILL_HOME_DIR` swaps the home folder UpSkill scans, and `UPSKILL_ROOT` moves its data. Use them for anything experimental.

## Build

```sh
bun run bundle         # → target/release/bundle/macos/UpSkill.app (+ dmg)
```

### Omarchy (Arch + Hyprland)

```sh
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator gtk3 base-devel
bun install && bun run bundle     # → AppImage / deb in target/release/bundle
```

Hyprland tiles new windows and Waybar must be up before the tray registers. In `~/.config/hypr/hyprland.conf`:

```
exec-once = sleep 2 && ~/Applications/UpSkill.AppImage
windowrulev2 = float, class:^(upskill-app)$
windowrulev2 = size 1240 800, class:^(upskill-app)$
```

On Linux the tray opens a menu rather than handling clicks (a Tauri limitation), so "Open UpSkill" is the first item.

## MCP

The app's Agents and Settings pages connect the MCP server with one click. By hand:

```sh
claude mcp add --scope user upskill -- /path/to/upskill mcp
codex mcp add upskill -- /path/to/upskill mcp
```

In the bundled app the binary is `UpSkill.app/Contents/MacOS/upskill`.

## CLI

```
upskill status | list | health | adopt-all | deploy | sync | ingest
upskill call <method> '<json>'      # any API method, e.g. call get_skill '{"name":"commit"}'
```
