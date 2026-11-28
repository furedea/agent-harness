# agent-harness

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/lang-English-8c959f"></a>
  <a href="README_ja.md"><img alt="日本語" src="https://img.shields.io/badge/lang-%E6%97%A5%E6%9C%AC%E8%AA%9E-0969da"></a>
</p>

**スキルだけでなくフックも，コーディングエージェント間で共有する．**

`agent-harness` は，フック，スキル，エージェント指示，コマンド権限，プロバイダ設定をまとめる Nix ネイティブの合成レイヤである．環境を 1 つ定義すれば，Claude Code，Codex，Devin，Hermes Agent，pi，DeepSeek Harness（dsh）が期待するネイティブなファイルを生成できる．

同梱のデフォルトは意図的に小さくしてある．個人やチームの振る舞いは，他の設定と並ぶ完全なソースツリーに置く．そこから，このリポジトリを fork せずに，ローカルや Nix でビルドしたスキルとフックバンドルを合成できる．

```text
AGENTS.md and command permissions
Local or command-generated skills
Local or command-generated hook bundles
Provider settings
              |
              v
        agent-harness
        |-- Claude Code: settings, hooks, and skills
        |-- Codex: config, hooks, adapters, rules, and skills
        |-- Devin: config, hooks, and command permissions
        |-- Hermes Agent: hook manifest, plugin, and config
        |-- pi: hook manifest, bridge extension, settings, and skills
        |-- dsh: hook manifest, bridge bundle, and skills
        `-- antigravity: hook manifest, skills, AGENTS.md, and generated settings permissions
```

## なぜ使うのか

- **フックの共有**: Claude Code，Codex，Devin，Hermes Agent，pi，dsh でフックの振る舞いを再利用する．通常は各プロバイダの設定ディレクトリに直接書き込む CLI のフックも対象にできる．
- **ガードレールを 1 つに**: Claude の Bash 権限，Codex の execpolicy，Devin の Exec 権限を，中立な `allow` / `ask` / `deny` の 1 つのソースから生成する．
- **合成できる拡張**: 組み込みの振る舞いに，外部のスキルとフックバンドルを組み合わせる．
- **CLI と Nix の橋渡し**: コマンドの出力をスキルにしたり，隔離したビルド用ホームでフックのインストーラが作るファイルを取り込んだりする．
- **プロバイダごとのネイティブ出力**: 各エージェントが期待するアダプタ，フック配線，権限，ルール，設定を生成する．
- **状態を壊さない設定の同期**: harness が管理するキーだけを置き換え，Claude Code，Codex，Devin，Hermes Agent，pi，dsh の設定でツールやユーザーが所有する状態は残す．

この設計が解決する問題，中核の設計判断，検討した代替案については [docs/DESIGN_ja.md](docs/DESIGN_ja.md) を参照．

## ソース

組み込みの `minimal` ソースには，中立な `AGENTS.md`，空のスキル・フック・権限セットが入っており，モデルのデフォルトは含まない．すぐ使える出発点であり，CLI のデフォルトでもある．リポジトリ作者の個人的なフックやスキルは含まない．

カスタマイズした環境は，`--source` で渡すか Home Manager モジュールの `source` に設定する，完全でフラットなソースツリーである．その `manifest.json` が必要な実行時コマンドを宣言し，`verify` が不足しているものを報告する．これにより，個人のポリシー変更はそれを所有するリポジトリに置かれ，agent-harness は再利用できるレンダラと配置の境界にとどまる．

## Nix で合成する

Home Manager モジュールは通常のパスと derivation を受け付ける．そのため，個人の設定をユーザーの他の Nix 設定の隣に置ける．

```text
dotfiles/
└── agents/
    ├── manifest.json
    ├── AGENTS.md
    ├── command_permissions.json
    ├── hooks.json
    ├── skill_rendering.json
    ├── hooks/
    ├── skills/
    ├── claude/
    └── codex/
```

```nix
programs.agent-harness = {
  enable = true;
  source = ./agents;

  skills = {
    team-workflows = ./agent-sources/skills/team-workflows;
  };

  hooks = {
    notifications = ./agent-sources/hook-bundles/notifications;
  };
};
```

flake は，他の CLI が所有する連携のためのビルダーも公開している．たとえば [Herdr](https://herdr.dev/) は，`herdr --skill` でリリースに合ったスキルを出力し，`herdr integration install` でプロバイダのフックをインストールする．

```nix
let
  harnessLib = agent-harness.lib.${pkgs.system};

  herdrSkill = harnessLib.buildSkillFromCommand {
    name = "herdr";
    command = [
      "${herdr}/bin/herdr"
      "--skill"
    ];
  };

  herdrHooks = harnessLib.buildHookBundleFromCommands {
    name = "herdr";
    commands = [
      [ "${herdr}/bin/herdr" "integration" "install" "claude" ]
      [ "${herdr}/bin/herdr" "integration" "install" "codex" ]
    ];
  };
in
{
  programs.agent-harness = {
    enable = true;
    skills.herdr = herdrSkill;
    hooks.herdr = herdrHooks;
  };
}
```

`buildSkillFromCommand` はコマンドの標準出力を `SKILL.md` に書き込み，空の出力は拒否する．`buildHookBundleFromCommands` は隔離した `HOME` でコマンドを順に実行し，対応するプロバイダのフック成果物を取り込んで，バージョン付きのバンドルを書き出す．各コマンドは引数のリストで，最初の要素が実行ファイル，残りがシェルを介さずに渡される引数である．フックのインストーラは絶対パスでなければならない．スキル用のコマンドにも Nix パッケージの絶対パスを推奨する．ビルドは決定的であるべきで，対話入力やネットワークアクセスに依存してはならない．

フックビルダーが取り込むのは，各プロバイダのフックファイル（`.claude/settings.json`，`.codex/hooks.json`，`.devin/hooks.v1.json`，`.hermes/hooks.json`，`.pi/hooks.json`，`.dsh/hooks.json`）とフックディレクトリ（`.claude/hooks/`，`.codex/hooks/`，`.devin/hooks/`，`.hermes/hooks/`，`.pi/hooks/`，`.dsh/hooks/`），加えて `.codex/config.toml` とトップレベルの `.codex/*.sh` である．生成されたフックのコマンドに，実行時には別の場所になるビルド時のパスが埋め込まれる場合は，`commandReplacements = [{ from = "..."; to = "..."; }]` を渡す．

[`moshi-hook install`](https://getmoshi.app/docs/hooks) のようなインストーラにも同じフックビルダーを使う．ペアリングの認証情報やデーモンの状態は実行時の関心事であり，バンドルには取り込まない．

```nix
moshiHooks = agent-harness.lib.${pkgs.system}.buildHookBundleFromCommands {
  name = "moshi";
  commands = [ [ "${moshiHook}/bin/moshi-hook" "install" ] ];
};
```

### GitHub の認証情報

agent-harness は GitHub のトークンを用意も更新もしない（[ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)）．エージェントが自分のトークンでサンドボックスの中から `gh` と `git push` を実行できるようにするには，配線をソースの設定に書く．harness はそれをそのまま生成する．Claude では `env.GH_CONFIG_DIR`，`sandbox.network.allowedDomains` の GitHub のホスト，`sandbox.enableWeakerNetworkIsolation`，`sandbox.excludedCommands` に `gh *` を含めないこと，読ませたくない鍵への `Read(...)` と `sandbox.filesystem.denyRead` の拒否を書く．Codex では `shell_environment_policy.set.GH_CONFIG_DIR` と，そのディレクトリを読める権限プロファイルを書く．トークンの発行と更新は harness の外のツールで行う．

### MCP サーバー

MCP サーバーは，ソースの任意のファイル `mcp_servers.json` に 1 回だけ宣言する．各サーバーはコマンド（`command` と，任意の `args`，`env`）か，リモートのエンドポイント（`url`，streamable HTTP）である．

```json
{
  "servers": {
    "github": {
      "command": "github-mcp-server",
      "args": ["stdio"],
      "env": { "GITHUB_TOOLSETS": "repos,issues" }
    },
    "docs": { "url": "https://docs.example.com/mcp" }
  }
}
```

コマンドは `PATH` 上の名前でよく，そうすればソースはホストに依存しない．Nix ストアのパスのようにホスト固有の値には，Home Manager のオプション `mcpServers` を使う．エントリを追加するか，ソースの同名のエントリを丸ごと置き換える．

```nix
programs.agent-harness.mcpServers.github = {
  command = "${pkgs.github-mcp-server}/bin/github-mcp-server";
  args = [ "stdio" ];
};
```

harness は値をそのままプロバイダのファイルに書くため，認証情報を書いてはならない．認証情報が必要なサーバーは自身の環境変数から読むか，`command` を，認証情報を（たとえばシステムのキーチェーンから）取り出してサーバーを起動するラッパースクリプトにする．

| プロバイダ  | 生成先                                                                 | リモートサーバーの書き方   | 自分で追加したサーバー                                    |
| ----------- | ---------------------------------------------------------------------- | -------------------------- | --------------------------------------------------------- |
| Claude Code | プラグイン `~/.claude/skills/agent-harness/.claude-plugin/plugin.json` | `type: "http"`，`url`      | `~/.claude.json` に残る．harness はこのファイルに触れない |
| Codex       | `~/.codex/config.toml` の `[mcp_servers.<name>]`                       | `url`                      | 残る．削除するのは前回の同期が書いたサーバーだけである    |
| Devin       | `~/.config/devin/mcp_config.json` の `mcpServers`                      | `url`，`transport: "http"` | 残る．削除するのは前回の同期が書いたサーバーだけである    |
| Hermes      | `~/.hermes/config.yaml` の `mcp_servers`                               | `url`                      | 残る．削除するのは前回の同期が書いたサーバーだけである    |
| Antigravity | `~/.gemini/config/mcp_config.json` の `mcpServers`                     | `serverUrl`                | 残る．削除するのは前回の同期が書いたサーバーだけである    |

Claude Code はスキルディレクトリのプラグインをその場で読み込み，書き換えない．そのため，同期が実行中のエージェントと `~/.claude.json` を取り合うことはない．ツール名は `mcp__plugin_agent-harness_<server>__<tool>` になるので，権限ルールはこの接頭辞で書く．`mcp_servers.json` とソースの `codex/config.toml` の両方にある名前はエラーになる．

Devin と Antigravity はサーバーを専用のファイルに持ち，`devin mcp add -s user` や Antigravity の MCP マネージャーもそこへ書く．インストールはこのファイルをプロバイダの設定の隣に生成し，`sync-config --provider <devin|antigravity> --file mcp-servers` が実際のファイルへマージする．Home Manager モジュールは activation でこの同期を実行する．同期が書いたサーバーは，Codex と同じく各ターゲットの隣の `.agent-harness-managed.json` に記録する．

Pi は MCP クライアントを持たず，agent-harness は Pi 用の MCP 拡張もインストールしないので，Pi にはサーバーを生成しない．dsh が MCP サーバーを使うには別パッケージの `@deepseek-ai/dsh-mcp-client` プラグインが必要で，各 dsh プロファイルが dsh に合うバージョンを自分でインストールする．agent-harness はこれをインストールしないので，dsh にも生成しない．[ADR-0033](docs/adr/0033_render_mcp_servers_from_one_declaration.md) を参照．

## クイックインストール

リリースバイナリは現在 `x86_64-unknown-linux-musl` 向けにだけビルドしている．そのプラットフォームで最も手早いのは，cargo-dist が生成するシェルインストーラである．

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-installer.sh \
  | sh
```

続いて，管理対象の harness ファイルをインストールし，検証する．

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

これは中立な組み込みの `minimal` ソースをインストールする．個人やチームの完全なソースツリーを生成するには，代わりに `--source <path>` を渡す．

ソースから Cargo でインストールする場合：

```bash
cargo install --locked --git https://github.com/furedea/agent-harness agent-harness
```

シェルと Cargo のインストーラは，バイナリを Cargo の bin ディレクトリ（通常は `$HOME/.cargo/bin`）に置く．そのディレクトリが `PATH` に入っていることを確認すること．

Apple Silicon の macOS で Nix を使う場合：

```bash
nix profile install github:furedea/agent-harness
```

バイナリをインストールした後も，プロバイダの設定を生成するには `agent-harness install` が必要である．`verify` は，必要なインストール先のパスと，解決したソースの manifest が宣言する実行時コマンドを確認する．

## その他のインストール方法

### リリースアーカイブ

`x86_64` の Linux でリリースバイナリを使いたいが，インストーラを Bash にパイプしたくない場合に使う．現在の cargo-dist の設定は，Unix 向けのデフォルトのアーカイブ形式 `.tar.xz` を使う．

```bash
mkdir -p "$HOME/.local/agent-harness" "$HOME/.local/bin"

curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz
tar -xJf agent-harness-x86_64-unknown-linux-musl.tar.xz \
  -C "$HOME/.local/agent-harness" \
  --strip-components=1

ln -sf "$HOME/.local/agent-harness/agent-harness" "$HOME/.local/bin/agent-harness"

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Cargo

Rust のツールチェーンがすでにあり，リポジトリからビルドしたい場合に使う．

```bash
cargo install --locked --git https://github.com/furedea/agent-harness agent-harness

agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

### Nix

flake は `x86_64-linux` と `aarch64-darwin` 向けにパッケージを公開している．

```bash
nix run github:furedea/agent-harness -- install --prefix "$HOME"
nix run github:furedea/agent-harness -- verify --prefix "$HOME"
```

### Home Manager

エージェントの設定を Nix で管理している場合は Home Manager モジュールを使う．モジュールは `homeManagerModules.default` として公開しており，`x86_64-linux` と `aarch64-darwin` に対応している．

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    agent-harness.url = "github:furedea/agent-harness";
  };

  outputs =
    {
      agent-harness,
      home-manager,
      nixpkgs,
      ...
    }:
    {
      homeConfigurations.example = home-manager.lib.homeManagerConfiguration {
        pkgs = import nixpkgs { system = "aarch64-darwin"; };
        modules = [
          agent-harness.homeManagerModules.default
          {
            programs.agent-harness.enable = true;
          }
        ];
      };
    };
}
```

[Nix で合成する](#nix-で合成する) で示したとおり，追加のスキルディレクトリや外部のフックバンドルは，このリポジトリにコピーせずに合成できる．スキルディレクトリは Claude，Codex，pi，dsh に展開され（Devin と Hermes にはスキルの出力先がない），フックバンドルはすべてのプロバイダに適用される．スキルディレクトリには `SKILL.md` が必要である．インストール名には小文字の ASCII 英字，数字，ハイフンだけが使え，組み込みのスキルを上書きする名前は使えない．通常のファイルはそのままコピーされ，シンボリックリンクは無視される．

各バンドルには `{"version":1}` を持つ `hook_bundle.json` が必要で，任意のプロバイダのフックファイルとフックスクリプト，加えて Codex の feature フラグを含められる．スクリプトは各プロバイダのフックディレクトリの下に `hooks/external/<name>/` としてインストールされる（例：`~/.claude/hooks/external/<name>/`）．フックの JSON は構造的にマージされ，外部の Codex 設定が追加できるのは `[features]` のエントリだけである．`hooks` に相当する CLI は，`install` と，`--source` と `--output` を取る `generate-*` コマンド（`generate-hook-bundle` と `generate-skills` を除くすべて）の `--extra-hook NAME=PATH` である．

モジュールのインターフェースの全体は次のとおり．

| オプション           | デフォルト             | 意味                                                                                                      |
| -------------------- | ---------------------- | --------------------------------------------------------------------------------------------------------- |
| `enable`             | `false`                | パッケージと管理対象ファイルを有効にする．                                                                |
| `package`            | flake のパッケージ     | `agent-harness` の実行ファイルを選ぶ．                                                                    |
| `source`             | この flake             | プロファイル集，または完全でフラットなプロファイルのソースツリー．                                        |
| `profile`            | `"minimal"`            | 同梱の合成のベースを選ぶ．                                                                                |
| `agentsMd`           | プロファイルのファイル | 共有の `AGENTS.md` / `CLAUDE.md` のソースを置き換える．                                                   |
| `commandPermissions` | プロファイルのファイル | プロバイダ中立な `allow` / `ask` / `deny` ルールを置き換える．                                            |
| `mcpServers`         | `{}`                   | MCP サーバーを追加するか，ソースの同名のサーバーを置き換える．                                            |
| `skills`             | `{}`                   | スキルのディレクトリや derivation をインストール名で追加する．                                            |
| `hooks`              | `{}`                   | バージョン付きフックバンドルのディレクトリや derivation を名前で追加する．                                |
| `claude.enable`      | `true`                 | Claude Code のファイルをインストールする．                                                                |
| `claude.settings`    | `{}`                   | 選んだ Claude のベース設定のキーを任意で上書きする．                                                      |
| `codex.enable`       | `true`                 | Codex のファイルをインストールする．                                                                      |
| `codex.settings`     | `{}`                   | 選んだ Codex のベース設定のキーを任意で上書きする．                                                       |
| `devin.enable`       | `true`                 | Devin のファイルをインストールする．                                                                      |
| `hermes.enable`      | `true`                 | Hermes Agent のファイルをインストールする．                                                               |
| `pi.enable`          | `true`                 | pi のファイルをインストールする．                                                                         |
| `antigravity.enable` | `true`                 | Antigravity のファイル（`~/.gemini/config`，`agy` の settings の権限）をインストールする．                |
| `dsh.enable`         | `true`                 | DeepSeek Harness のファイルをインストールする．                                                           |
| `dsh.profile`        | `"web"`                | フックバンドルを登録する，ユーザー所有の dsh プロファイル．                                               |

生成されたフック配線と Bash のコマンド権限は，プロバイダ設定のマージ後も harness の管理下に残る．これにより，設定のオーバーレイが強制力を黙って取り除くことを防ぐ．harness はインストールしたファイルを保護しない．保護はソースの設定に書く．たとえばサンドボックスの書き込みの許可リストと，`Edit(~/.claude/**)` のようなハーネスのルートごとの拒否である（[ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)）．プロバイダの `enable` を `false` にすると，そのプロバイダのファイルはインストールされない．

Claude Code が `~/.claude/settings.json` を更新することがあるため，Home Manager モジュールはこのファイルを書き込み可能な通常のファイルとして実体化する．activation のたびに，生成されたトップレベルのキーが既存の対応する値を置き換え，生成された設定にないトップレベルのキーは残る．その他の Claude Code のファイルは，Home Manager が管理するシンボリックリンクのままである．activation では，[インストールされるもの](#インストールされるもの) で説明する Codex，Devin，Hermes Agent，pi，dsh の設定の同期も行う．

## 任意の完全性チェック

cargo-dist は，リリースアーカイブごとに `.sha256` ファイルを公開する．

```bash
curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz
curl -fsSLO \
  https://github.com/furedea/agent-harness/releases/latest/download/agent-harness-x86_64-unknown-linux-musl.tar.xz.sha256
sha256sum -c agent-harness-x86_64-unknown-linux-musl.tar.xz.sha256
```

## インストールされるもの

`agent-harness install --prefix "$HOME"` は，生成した harness を Claude Code，Codex，Devin，Hermes Agent，pi，dsh の設定ディレクトリに書き込む．

| パス                                                 | 用途                                                 |
| ---------------------------------------------------- | ---------------------------------------------------- |
| `~/.codex/AGENTS.md`                                 | Codex のエージェント指示                             |
| `~/.codex/config.toml`                               | 管理対象の Codex 設定                                |
| `~/.codex/hooks.json`                                | Codex のフック配線                                   |
| `~/.codex/hooks/`                                    | Codex のフックアダプタ                               |
| `~/.codex/rules/default.rules`                       | Codex のコマンド権限                                 |
| `~/.codex/skills/`                                   | 生成した Codex のスキル                              |
| `~/.claude/CLAUDE.md`                                | Claude Code のエージェント指示                       |
| `~/.claude/settings.json`                            | Claude Code の設定                                   |
| `~/.claude/hooks/`                                   | Claude Code のフックとポリシーガード                 |
| `~/.claude/skills/`                                  | 生成した Claude Code のスキル                        |
| `~/.claude/skills/agent-harness/`                    | MCP サーバーを宣言する Claude Code のプラグイン      |
| `~/.claude/statusline/`                              | Claude Code のステータスラインコマンド               |
| `~/.devin/hooks/`                                    | Devin のフックアダプタ                               |
| `~/.config/devin/config.json`                        | Devin のフックとコマンド権限                         |
| `~/.config/devin/mcp_config.json`                    | Devin の MCP サーバー．実際のファイルへ同期する      |
| `~/.hermes/hooks.json`                               | Hermes Agent のフックマニフェスト                    |
| `~/.hermes/hooks/`                                   | Hermes Agent のフックアダプタ                        |
| `~/.hermes/plugins/agent-harness-hooks/`             | マニフェストを実行する Hermes Agent のプラグイン     |
| `~/.hermes/managed-config.yaml`                      | `config.yaml` に同期する Hermes Agent のキー         |
| `~/.pi/agent/AGENTS.md`                              | 共有のエージェント指示                               |
| `~/.pi/agent/skills/`                                | Claude 互換の frontmatter を持つスキル               |
| `~/.pi/agent/settings.json`                          | 管理対象のモデルのデフォルトをユーザーのキーとマージ |
| `~/.pi/agent/hooks.json`                             | pi のフックマニフェスト                              |
| `~/.pi/agent/extensions/hook_bridge.ts`              | マニフェストを実行する pi の拡張                     |
| `~/.pi/hooks/`                                       | pi のフックアダプタ                                  |
| `~/.dsh/AGENTS.md`                                   | dsh のエージェント指示                               |
| `~/.dsh/hooks.json`                                  | dsh のフックマニフェスト                             |
| `~/.dsh/hooks/`                                      | dsh のフックアダプタ                                 |
| `~/.dsh/bridge/`                                     | マニフェストを実行する dsh のブリッジバンドル        |
| `~/.dsh/skills/`                                     | Codex の生成結果からコピーしたスキル                 |
| `~/.gemini/config/mcp_config.json`                   | Antigravity の MCP サーバー．同じく同期する          |

agent-harness は Pi に OS レベルの隔離を与えない．隔離が必要なときは，Pi を自分で用意したサンドボックスの中で動かす（[ADR-0032](docs/adr/0032_limit_the_harness_to_rendering_and_installing.md)）．インストールのたびに，以前のリリースが `~/.pi/agent/extensions/` に置いた srt 拡張のファイル（`bash_sandbox.ts`，`sandbox_policy.json`，`sandbox/file_operations.mjs`）を削除する．自分の拡張は別の名前で置く．フックは方針の層にすぎない．Claude と同様に，フックが失敗すると呼び出しは続行される．

インストールは，管理対象のフックとスキルのディレクトリを置き換え，Claude Code の管理対象の設定を書き直す．Codex の設定の同期は次の管理対象トップレベルキーだけを置き換え，プロジェクトの信頼情報やマーケットプレイスのデータなど，Codex やユーザーが所有する他の状態は残す．

```text
model, model_reasoning_effort, personality, approval_policy, sandbox_mode,
approvals_reviewer, cli_auth_credentials_store, notice, tui, plugins, features,
default_permissions, permissions
```

さらに，ソースが `analytics`，`otel`，`feedback`，`check_for_update_on_startup` を設定している場合はそれらを書き込み，設定していない場合は削除せずそのまま残す．

また，ソースの `shell_environment_policy.set` の下にあるキーをすべて書き込み，前回の同期が書いたキーのうちソースから消えたものを削除する．記録はターゲットの隣の `.agent-harness-managed.json` にある．Codex やユーザーが追加したキーは残る．サイドカーがない場合は，ソースに `GH_CONFIG_DIR` がないときだけ `GH_CONFIG_DIR` を削除する．`mcp_servers` のエントリも同じように記録するため，Codex やユーザーが追加したサーバーは残る．

生成される Claude の設定は，ソースの `claude/settings.json` が宣言する `permissions.allow`，`permissions.ask`，`permissions.deny` のエントリをすべて残し，その後に生成した `Bash(...)`，`Read(...)`，`Edit(...)` のエントリを続ける．各ルールは 1 回だけ現れ，複数のリストで宣言されたルールは最も厳しいリスト（`deny` > `ask` > `allow`）にだけ残る．そのため，マージによって権限が広がることはない．`defaultMode` などの他の権限キーはそのまま引き継がれる．Claude の設定の同期は生成された `permissions` キー全体を置き換えるため，Claude Code が対話的に `~/.claude/settings.json` に保存した承認は次の同期で残らない．永続させたいルールは `command_permissions.json` かソースの `claude/settings.json` に書くこと．

Devin の設定の同期は管理対象のトップレベルキーを置き換え，`permissions` の中では `Exec(...)` のエントリだけを置き換える．`Read(...)`，`Write(...)`，`Fetch(...)`，`mcp__*` などの Exec 以外のルールはユーザーの所有のままである．対話的に `Exec(...)` として保存した承認は次の同期で置き換わるので，永続させたいコマンドルールは `command_permissions.json` に書くこと．

`install` は `~/.hermes/config.yaml` を編集しない．`sync-config --provider hermes` が `~/.hermes/managed-config.yaml` をそこへマージする．生成された `plugins.enabled` のエントリは重複なく追加され，`mcp_servers` のエントリは Codex と同じく記録して同期し，Hermes Agent の他の状態は残る．Home Manager モジュールは activation でこの同期を実行する．

`sync-config --provider dsh` は，ユーザー所有の dsh プロファイルの `~/.dsh/profiles/<profile>/package.json` にブリッジバンドルを登録する．プロファイル名は Home Manager のオプション `dsh.profile` で指定し（既定は `web`）．この同期が管理するのは `dependencies` のエントリで，`dsh.profile.bundles` にはバンドルを 1 回だけ追加する．dsh がまだ作成していないプロファイルはスキップする．ブリッジの設計は [ADR-0025](docs/adr/0025_embed_the_dsh_hook_bridge.md) に記録している．

`verify` は，解決したソースの manifest が宣言する必要なパスと実行時コマンドを確認する．インストールされたファイルの内容を選んだソースと比較はしない．

## 使い方

ほとんどの利用者に必要なのは次だけである．

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

プロジェクトローカルにインストールする場合は，出力先のプレフィックスと実行時ルートの両方を指定する．`--prefix` はファイルを書き込む場所を，`--runtime-root` は生成したフックのコマンドが実行時にファイルを期待する場所を決める．実行時ルートは絶対パスでなければならない．

```bash
agent-harness install \
  --source /absolute/path/to/agents \
  --prefix "$PWD" \
  --runtime-root "$PWD"
agent-harness verify --source /absolute/path/to/agents --prefix "$PWD"
```

デフォルトの実行時ルートは引き続きユーザーのホームディレクトリなので，既存のユーザー単位のインストールに追加のオプションは要らない．移し替えたフックのコマンドは，選んだディレクトリを `AGENT_HARNESS_ROOT` として公開する．隣接する harness の資産を呼び出すソース側のアダプタは，`${AGENT_HARNESS_ROOT:-$HOME}` からそれらを解決すること．

解決した harness のソースが管理する組み込みのコンポーネントを確認する．

```bash
agent-harness list
agent-harness list skills
agent-harness list hooks
agent-harness list skills --source ./agents
agent-harness list hooks --provider codex --source ./agents
```

CLI は，個々の出力を確認したり合成したりするための低レベルの生成コマンドも公開している．

| コマンド                         | 出力                                             |
| -------------------------------- | ------------------------------------------------ |
| `generate-hooks --provider <id>` | プロバイダのフック JSON または設定断片           |
| `sync-config --provider <id>`    | 既存の設定へ管理対象を同期                       |
| `sync-config --provider <id> --file mcp-servers` | Devin と Antigravity の専用ファイルへ MCP サーバーを同期 |
| `generate-claude-settings`       | Claude Code の設定 JSON 全体                     |
| `generate-codex-config-source`   | 管理対象の Codex 設定ソース全体                  |
| `generate-codex-config-fragment` | ガード付きファイルシステムの TOML 断片           |
| `generate-codex-rules`           | Codex の execpolicy ルール                       |
| `generate-command-permissions`   | 共有の実行時コマンド権限                         |
| `generate-forbidden-commands`    | グローバルで厳密な禁止コマンドの正規表現ルール   |
| `generate-hook-bundle`           | 隔離して作るバージョン付きの外部フックバンドル   |
| `generate-skills --provider <id>` | プロバイダのスキルディレクトリ（組み込みと外部のスキル） |

`generate-hooks` は `claude`，`codex`，`devin`，`hermes`，`pi`，`dsh` に対応する．`sync-config` も同じ指定を受け付ける．pi については，生成された設定から `defaultProvider`，`defaultModel`，`defaultThinkingLevel` だけをマージし，ユーザー所有のキーは残す．任意のソースのデフォルトは `pi/settings.json` に置く．Pi のスキルは `disable-model-invocation` を含む Claude の形式で生成し，`~/.pi/agent/skills/` の下にインストールする．共有の指示は `~/.pi/agent/AGENTS.md` としてインストールする．

Pi のフックの判定は deny > ask > allow の順に優先される．ask の判定は，UI があればツールの入力とともに確認を求め，「Yes」の場合だけ実行を許可する．UI がなければ ask はブロックする．従来の `generate-<provider>-hooks`，`sync-claude-settings`，`sync-{codex,devin,hermes,dsh}-config` の各コマンドは，非推奨の別名として引き続き動作し，ヘルプには表示しない．

```bash
agent-harness generate-skills \
  --provider codex \
  --extra-skill external-tool=/path/to/external-tool \
  --output "$HOME/.codex/skills"

agent-harness generate-skills \
  --provider claude \
  --output "$HOME/.claude/skills"

agent-harness generate-hook-bundle \
  --spec /path/to/hook-bundle-spec.json \
  --output /path/to/hook-bundle

agent-harness generate-hooks --provider codex \
  --output "$HOME/.codex/hooks.json"

agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"

agent-harness generate-command-permissions \
  --output "$HOME/.claude/hooks/rules/command_permissions.json"

agent-harness generate-forbidden-commands \
  --output "$HOME/.claude/hooks/rules/forbidden_commands.json"

agent-harness generate-codex-config-source \
  --output /tmp/codex-config-source.toml

agent-harness sync-config --provider codex \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"

agent-harness generate-claude-settings \
  --output "$HOME/.claude/settings.json"
```

フックバンドルの spec には，絶対パスのインストーラ実行ファイルとその引数を並べる．インストーラは隔離した一時ホームで実行され，対応するプロバイダのフック成果物だけが取り込まれる．任意のコマンド置換を使うと，ビルド時の実行ファイルのパスを安定した実行時のパスに変えられる．

```json
{
  "version": 1,
  "installers": [
    {
      "executable": "/nix/store/example/bin/example-hook",
      "arguments": ["install", "--target", "claude,codex"]
    }
  ],
  "command_replacements": [
    {
      "from": "/nix/store/example/bin/example-hook",
      "to": "/opt/homebrew/bin/example-hook"
    }
  ]
}
```

## カスタマイズ

Nix で管理するカスタマイズでは，完全でフラットなソースツリーか，上記の Home Manager の直接のオプションを使うのがよい．カスタムのツリーは `agent-harness install --source <path> --prefix "$HOME"` で生成する．

| 目的                                             | 編集するもの                                           |
| ------------------------------------------------ | ------------------------------------------------------ |
| 実行時コマンドの要件を宣言する                   | `agents/manifest.json`                                 |
| 共有のエージェント指示を変える                   | `agents/AGENTS.md`                                     |
| スキルを追加・編集する                           | `agents/skills/<skill>/SKILL.md`                       |
| プロバイダ別のスキルのメタデータを変える         | `agents/skill_rendering.json`                          |
| 共有のコマンド権限を変える                       | `agents/command_permissions.json`                      |
| グローバルで厳密なコマンド形式を変える           | `agents/hooks/rules/{allowed,forbidden}_commands.json` |
| フック配線を追加・変更する                       | `agents/hooks.json`                                    |
| Claude のフックを追加・変更する                  | `agents/hooks/*.sh`                                    |
| Codex のフックアダプタを追加・変更する           | `agents/codex/hooks/*.sh`                              |
| Devin，Hermes，pi，dsh のフックを追加・変更する  | `agents/{devin,hermes,pi,dsh}/hooks/`                  |
| シークレットパスまたはコミットのポリシーを変える | `agents/hooks/rules/secret_{path,commit}_policy.json`  |
| pi のモデルのデフォルトを変える                  | `agents/pi/settings.json`                              |
| Codex のベース設定を変える                       | `agents/codex/config.toml`                             |
| Claude のベース設定を変える                      | `agents/claude/settings.json`                          |

コマンド権限は 2 層になっている．プロファイルのルートにある `command_permissions.json` は，共有するトークンのプレフィックスを定義するプロバイダ中立なソースである．各ルールは `decision`（`allow`，`ask`，`deny` のいずれか），`prefix`，必須の justification を持つ．

```json
{
  "version": 1,
  "rules": [
    {
      "decision": "ask",
      "prefix": ["git", "push"],
      "justification": "Publishing changes requires user confirmation."
    }
  ]
}
```

ジェネレータはこれらのルールを，Claude Code の `allow`，`ask`，`deny` の Bash 権限，Codex の `allow`，`prompt`，`forbidden` の execpolicy の判定，Devin の `Exec(...)` の `allow`，`ask`，`deny` の権限に対応させる．`hooks/rules/` の下にある `allowed_commands.json` と `forbidden_commands.json` には，グローバルで厳密なコマンド形式を表す POSIX 拡張正規表現を書く．

```json
{
  "version": 1,
  "rules": [
    {
      "programs": ["uv"],
      "patterns": ["^uv run --frozen example$"],
      "exceptions": ["^uv run --frozen example --unsafe$"],
      "justification": "Allow the repository's validated example task."
    }
  ]
}
```

`programs` と `exceptions` は省略できる．agent-harness はこれらのファイルを検証し，フックの隣にインストールする．実行時にこれらを強制するのは，プロジェクトローカルのルールファイルを含めて，ソースのフックスクリプトの役目である．組み込みの `minimal` ソースにはそのようなフックは含まれない．

`hooks/rules/secret_path_policy.json` と `hooks/rules/secret_commit_policy.json` も必須である．読み取りを拒否するシークレットパスのルールは，Codex の `[permissions.guarded.filesystem]` の `"none"` のエントリにもなる．パターンのルールは [docs/DESIGN_ja.md](docs/DESIGN_ja.md) を参照．

インストールしたバイナリをローカルのソースツリーと使う：

```bash
agent-harness install --source "$HOME/dotfiles/agents" --prefix "$HOME"
```

ローカルで繰り返し生成する場合：

```bash
export AGENT_HARNESS_SOURCE="$HOME/dotfiles/agents"
agent-harness install --prefix "$HOME"
```

## ソースの解決順

`--source` を省略すると，`agent-harness` は次の順に資産を解決する．

1. 明示した `--source`
2. `AGENT_HARNESS_SOURCE`
3. バイナリのディレクトリの下の `share/agent-harness`
4. バイナリのインストールプレフィックスの下の `share/agent-harness`
5. カレントディレクトリが `agent-harness` のソースツリーであればそこ
6. 埋め込まれた同梱の資産

ソースのルートを解決した後，CLI は `profiles/<profile>` があればそれを選ぶ．なければ，ルートを完全でフラットなプロファイルとして扱う．そのプロファイルには `manifest.json`，`AGENTS.md`，`command_permissions.json`，`hooks.json`，`skill_rendering.json`，`claude/settings.json`，`codex/config.toml`，`hooks/rules/{allowed_commands,forbidden_commands,secret_commit_policy,secret_path_policy}.json` が必要である．`skills/` と他のプロバイダのフックディレクトリは省略できる．これにより，dotfiles リポジトリは `agents/` を直接公開でき，同じバイナリがリリースの tarball，Nix のビルド，Cargo のインストール，ローカルのチェックアウトのどれでも動き続ける．

## コントリビュート

開発環境，ローカルのチェックアウトでのコマンド，品質ゲート，リリースの手順は [CONTRIBUTING.md](CONTRIBUTING.md) を参照．
