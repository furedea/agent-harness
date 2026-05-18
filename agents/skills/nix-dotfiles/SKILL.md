---
name: nix-dotfiles
description: >
    Nix configuration reference for nix-darwin + home-manager on Apple Silicon macOS. ALWAYS load when the user says "dotfiles" — dotfiles here means nix-managed config. Load for: adding packages, configuring programs, dotfile symlinks, macOS system defaults, activation scripts, unfree packages, darwin-rebuild, home-manager switch, homebrew casks/brews, or any macOS app preference/shortcut change (app prefs like Homerow, Raycast, Ghostty are managed via system.defaults in nix/darwin/default.nix, NOT edited directly). Skip only for standalone non-.nix config files (karabiner.json, starship.toml, nvim/) needing no nix rebuild. Trigger on: "dotfiles", flake.nix, nix/home/default.nix, nix/darwin/default.nix, nixpkgs, home-manager, programs.*, homebrew.casks, homebrew.brews, mkOutOfStoreSymlink, allowUnfree, darwin-rebuild, system.defaults, "nix で管理", "add to nix", app shortcut/preference updates.
---

# Nix Configuration — Dotfiles Reference

## Architecture

```
~/ghq/github.com/furedea/dotfiles/
├── flake.nix                        # entry point: inputs + outputs
│   ├── darwinConfigurations."mba"   # full system (nix-darwin + home-manager)
│   └── homeConfigurations."kaito"   # standalone home-manager (no sudo)
├── nix/darwin/default.nix           # system layer (nix-darwin)
│   ├── system.defaults.*            # macOS settings
│   ├── homebrew.*                   # GUI apps, taps, MAS
│   ├── nix-homebrew                 # homebrew installation via nix
│   └── system.activationScripts     # root-level run-once shell scripts
└── nix/home/default.nix             # user layer (home-manager)
    ├── home.packages                # CLI tools from nixpkgs / unstable / flake inputs
    ├── programs.*                   # declarative program configs (git, delta, gh, direnv, atuin, yazi, tmux)
    ├── home.file.*                  # dotfile symlinks (mkOutOfStoreSymlink)
    └── home.activation              # post-activation hooks (rustup, uv, ssh-keygen)
```

Platform: `aarch64-darwin` (Apple Silicon). Release channel: `25.11` for nixpkgs, nix-darwin, and home-manager. `nixpkgs-unstable` is available as `unstable` overlay for packages that need bleeding-edge versions.

## Flake Inputs

| Input | URL | Purpose |
| --- | --- | --- |
| `nixpkgs` | `nixpkgs-25.11-darwin` | Primary package set |
| `nixpkgs-unstable` | `nixpkgs-unstable` | Packages not yet in 25.11 (e.g. `atuin`, `oxfmt`, `oxlint`, `tsgolint`) |
| `nix-darwin` | `nix-darwin-25.11` | macOS system management |
| `home-manager` | `release-25.11` | User-level config management |
| `nix-homebrew` | `nix-homebrew` | Declarative Homebrew installation |
| `nix-claude-code` | `ryoppippi/nix-claude-code` | Claude Code CLI (unfree) |
| `codex-cli-nix` | `sadjow/codex-cli-nix` | Codex CLI |

Third-party flake inputs are accessed via `extraSpecialArgs` in home-manager: `nix-claude-code.packages.${system}.default` and `codex-cli-nix.packages.${system}.default`.

## Rebuild Commands

Two build paths exist — pick the right one for the change being made.

### `darwin-rebuild switch` (full system)

```bash
sudo darwin-rebuild switch --flake "$HOME/ghq/github.com/furedea/dotfiles/#mba"
```

Rebuilds **both** nix-darwin (system.defaults, homebrew, activationScripts) and home-manager (packages, programs, dotfile symlinks). Requires `sudo` because nix-darwin writes to `/etc/` and `/Library/Preferences/`. Use this for:

- Changes to `nix/darwin/default.nix` (macOS defaults, homebrew casks/brews, activation scripts)
- Changes to `flake.nix` (inputs, overlays, unfree list)
- Any change that touches both layers

### `home-manager switch` (user only, no sudo)

```bash
home-manager switch --flake "$HOME/ghq/github.com/furedea/dotfiles/#kaito"
```

Rebuilds **only** home-manager (packages, programs, dotfile symlinks, activation hooks). Does not touch nix-darwin or require `sudo`. Use this for:

- Changes to `nix/home/default.nix` only (adding packages, editing programs.\*, updating dotfile symlinks, activation hooks)
- Faster iteration when the darwin layer is unchanged

The `homeConfigurations."kaito"` output in `flake.nix` makes this possible — it imports `nix/home/default.nix` directly without going through nix-darwin.

### When symlinked dotfiles change

Changes to files linked via `mkOutOfStoreSymlink` (e.g. `.zshrc`, `starship.toml`, `nvim/`) take effect immediately — no rebuild needed. The symlink points to the working tree, not the nix store.

## Where to Add a Package

| What | Where | Example |
| --- | --- | --- |
| CLI tool in nixpkgs stable | `nix/home/default.nix` → `home.packages` | `bat`, `ripgrep` |
| CLI tool only in unstable | `nix/home/default.nix` → `home.packages` | `unstable.atuin`, `unstable.oxfmt` |
| Tool from a flake input | `nix/home/default.nix` → `home.packages` | `nix-claude-code.packages.${system}.default` |
| GUI app (macOS .app) | `nix/darwin/default.nix` → `homebrew.casks` | `"obsidian"`, `"arc"` |
| Mac App Store app | `nix/darwin/default.nix` → `homebrew.masApps` | `LINE = 539883307` |
| Homebrew formula (not in nixpkgs) | `nix/darwin/default.nix` → `homebrew.brews` | `"winebarrel/kasa/kasa"` |
| Homebrew tap | `nix/darwin/default.nix` → `homebrew.taps` | `"winebarrel/kasa"` |
| System-wide tool (before login) | `nix/darwin/default.nix` → `environment.systemPackages` | `pkgs.vim` |

### Package organization in home.packages

Packages are grouped by category with comments. Follow the existing grouping:

```nix
home.packages = with pkgs; [
  # Shell utilities
  # File operations
  # Dev tools
  # AI Coding Agent          ← flake inputs + unstable
  # General Formatters
  # Nix tools
  # ShellScript tools(bash)
  # Python tools
  # Rust tools
  # TypeScript tools          ← some from unstable
  # Lua tools
  # LaTeX tools
  # macOS-specific
];
```

## Allowing Unfree Packages

The `allowUnfreePredicate` is defined once in `flake.nix` and shared across both `pkgs` and `unstable`:

```nix
allowUnfreePredicate = pkg:
  builtins.elem pkg.pname [
    "zsh-abbr"           # CC-BY-NC-SA-4.0 + Hippocratic License v3.0
    "claude"             # Anthropic proprietary (via nix-claude-code)
    "github-copilot-cli" # GitHub proprietary
  ];
```

To add a new unfree package: find its `pname` with `nix eval nixpkgs#pkg-name.pname`, then add it to the list.

## Configuring Programs (home-manager modules)

Currently configured programs:

| Program | Key settings |
| --- | --- |
| `programs.git` | SSH signing, histogram diff, rerere, autoStash rebase, fsmonitor |
| `programs.delta` | Side-by-side, Catppuccin Mocha colors, line numbers |
| `programs.gh` | HTTPS protocol, `co` alias for PR checkout |
| `programs.direnv` | `nix-direnv.enable = true`, zsh integration disabled (manual in .zshrc) |
| `programs.atuin` | `package = unstable.atuin`, enter_accept, sync.records |
| `programs.yazi` | show_hidden, custom keybindings (o=create, Esc=quit) |
| `programs.tmux` | Mouse, extended-keys, vim-tmux-navigator, Catppuccin pane dimming |

All programs with shell integration have `enableZshIntegration = false` because `.zshrc` is a `mkOutOfStoreSymlink` dotfile — add `eval "$(tool init zsh)"` manually in `zsh/.zshrc`.

## Dotfile Symlinks

### Editable symlinks (mkOutOfStoreSymlink)

```nix
let
  link = path: config.lib.file.mkOutOfStoreSymlink "${dotfilesDir}/${path}";
in
{
  home.file = {
    ".zshrc".source    = link "zsh/.zshrc";
    ".config/nvim".source = link "nvim";       # directory symlink
    # ... etc
  };
}
```

Current symlinks cover: zsh (.zshrc, .zshenv, .zprofile), bash (.bashrc), git (ignore), nvim, starship, vim, dprint, prettier, ghostty, karabiner, cmux, Claude Code (.claude/), and Codex (.codex/).

Use `mkOutOfStoreSymlink` for files edited frequently — changes apply immediately without rebuild.

### Nix-generated files

```nix
home.file.".config/zsh/nix-plugins.zsh".text = ''
  source ${pkgs.zsh-autosuggestions}/share/zsh-autosuggestions/zsh-autosuggestions.zsh
'';
```

Use `.text` when the file content depends on Nix store paths (e.g. plugin source paths that change on every nixpkgs update).

## Activation Hooks

Run commands after home-manager writes files:

```nix
home.activation = {
  rustupInit = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.rustup}/bin/rustup toolchain install stable --no-self-update 2>/dev/null || true
  '';
  uvPythonInstall = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.uv}/bin/uv python install 2>/dev/null || true
  '';
  sshKeyGen = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    if [ ! -f ~/.ssh/id_ed25519 ]; then
      mkdir -p ~/.ssh
      ssh-keygen -t ed25519 -C "132188853+furedea@users.noreply.github.com" -f ~/.ssh/id_ed25519 -N ""
    fi
  '';
};
```

- Use `${pkgs.xxx}/bin/xxx` to reference the exact Nix store binary.
- End with `|| true` to prevent rebuild failures from non-fatal errors.
- `entryAfter [ "writeBoundary" ]` ensures dotfiles are written before the hook runs.
- `sshKeyGen` is idempotent — only runs if the key file does not exist.

## macOS System Defaults (nix-darwin)

Managed in `nix/darwin/default.nix` → `system.defaults`. Current categories:

| Category | Key settings |
| --- | --- |
| `NSGlobalDomain` | Fast key repeat, F1-F12 as standard, disable auto-correct/capitalize/quotes/dashes/periods, Dark mode |
| `finder` | Show all files, path bar, status bar, column view, folders first |
| `dock` | Autohide, 128px tiles, no recents, hot corners (Desktop/Notification Center/Lock Screen/Quick Note), persistent-apps list |
| `trackpad` | Tap to click, light click threshold, momentum scroll, pinch/rotate |
| `screencapture` | Save to ~/Pictures |
| `WindowManager` | Stage Manager disabled |
| `screensaver` | Password immediately |
| `menuExtraClock` | 24-hour, seconds, date, day of week |
| `CustomUserPreferences` | Spotlight disabled (Raycast), no .DS_Store on network/USB, Homerow config, mouse speed |

For keys not covered by native nix-darwin options, use `system.defaults.CustomUserPreferences."com.bundle.id"`.

## System Activation Scripts

Root-level scripts in `nix/darwin/default.nix`:

- **preActivation**: Back up Apple-provided `/etc/{bashrc,zshrc,zshenv,zprofile}` before nix-darwin's hash check (fixes "Unexpected files in /etc" on fresh Macs)
- **postActivation**: Disable Apple Music auto-launch (`rcd`), disable Spotlight shortcut (using Raycast), disable automatic macOS updates, set display sleep to never

## Homebrew (via nix-homebrew)

```nix
nix-homebrew = {
  enable = true;
  user = username;
  autoMigrate = true;
};

homebrew = {
  enable = true;
  onActivation = {
    autoUpdate = true;
    upgrade = true;
    cleanup = "uninstall";   # remove unlisted casks/brews on rebuild
  };
  casks = [ ... ];           # GUI apps
  taps = [ ... ];            # third-party repos
  brews = [ ... ];           # formulae not in nixpkgs
};
```

`cleanup = "uninstall"` means any cask or brew removed from the list will be uninstalled on the next `darwin-rebuild switch`. This keeps the machine declarative.

## Key Patterns

| Pattern | Usage |
| --- | --- |
| `with pkgs;` | Avoids repeating `pkgs.` in `home.packages` list |
| `unstable.xxx` | Package from `nixpkgs-unstable` (passed via `extraSpecialArgs`) |
| `input.packages.${system}.default` | Package from a third-party flake input |
| `link "path"` | Helper for `mkOutOfStoreSymlink` (defined in `let` block) |
| `${pkgs.xxx}` in `.text` | Embeds Nix store paths into generated files |
| `enableZshIntegration = false` | All programs — `.zshrc` is a symlink, hook manually |
| `\|\| true` in activation | Prevents non-fatal errors from aborting rebuild |
| `homebrew.onActivation.cleanup = "uninstall"` | Declarative cask management |
