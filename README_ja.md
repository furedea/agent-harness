# agent-harness

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/lang-English-8c959f"></a>
  <a href="README_ja.md"><img alt="日本語" src="https://img.shields.io/badge/lang-%E6%97%A5%E6%9C%AC%E8%AA%9E-0969da"></a>
</p>

**スキルだけでなくフックも，コーディングエージェント間で共有する．**

`agent-harness` は，フック，スキル，エージェント指示，コマンド権限，プロバイダ設定をまとめる Nix ネイティブの合成レイヤである．環境を 1 つ定義すれば，Claude Code，Codex，Devin，Hermes Agent，pi が期待するネイティブなファイルを生成できる．

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
        `-- pi: hook manifest and bridge extension
```

## なぜ使うのか

- **フックの共有**: Claude Code，Codex，Devin，Hermes Agent，pi でフックの振る舞いを再利用する．通常は各プロバイダの設定ディレクトリに直接書き込む CLI のフックも対象にできる．
- **ガードレールを 1 つに**: Claude の Bash 権限，Codex の execpolicy，Devin の Exec 権限を，中立な `allow` / `ask` / `deny` の 1 つのソースから生成する．
- **合成できる拡張**: 組み込みの振る舞いに，外部のスキルとフックバンドルを組み合わせる．
- **CLI と Nix の橋渡し**: コマンドの出力をスキルにしたり，隔離したビルド用ホームでフックのインストーラが作るファイルを取り込んだりする．
- **プロバイダごとのネイティブ出力**: 各エージェントが期待するアダプタ，フック配線，権限，ルール，設定を生成する．
- **状態を壊さない設定の同期**: harness が管理するキーだけを置き換え，Claude Code，Codex，Devin，Hermes Agent の設定でツールやユーザーが所有する状態は残す．

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

`buildSkillFromCommand` はコマンドの標準出力を `SKILL.md` に書き込み，空の出力は拒否する．`buildHookBundleFromCommands` は隔離した `HOME` でコマンドを順に実行し，対応する Claude と Codex の成果物を取り込んで，バージョン付きのバンドルを書き出す．各コマンドは引数のリストで，最初の要素が実行ファイル，残りがシェルを介さずに渡される引数である．フックのインストーラは絶対パスでなければならない．スキル用のコマンドにも Nix パッケージの絶対パスを推奨する．ビルドは決定的であるべきで，対話入力やネットワークアクセスに依存してはならない．

フックビルダーが取り込むのは，`.claude/settings.json`，`.claude/hooks/`，`.codex/hooks.json`，`.codex/hooks/`，`.codex/config.toml`，トップレベルの `.codex/*.sh` である．生成されたフックのコマンドに，実行時には別の場所になるビルド時のパスが埋め込まれる場合は，`commandReplacements = [{ from = "..."; to = "..."; }]` を渡す．

[`moshi-hook install`](https://getmoshi.app/docs/hooks) のようなインストーラにも同じフックビルダーを使う．ペアリングの認証情報やデーモンの状態は実行時の関心事であり，バンドルには取り込まない．

```nix
moshiHooks = agent-harness.lib.${pkgs.system}.buildHookBundleFromCommands {
  name = "moshi";
  commands = [ [ "${moshiHook}/bin/moshi-hook" "install" ] ];
};
```

### Claude サンドボックス内のエージェント用 GitHub トークン

既定では，ユーザーの GitHub トークンはサンドボックスの外でしか読めない．そのため Claude のソース設定は `gh *` を `sandbox.excludedCommands` に列挙する．gh を他のコマンドと連結した呼び出しはサンドボックス内で実行され，認証に失敗して承認プロンプトで止まる．オプトインの `github.agentToken` オプションは，サンドボックス内で読めるエージェント専用のトークンを Claude Code に与える．

1. エージェントが変更してよいリポジトリに限定した [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new) を作成する．Contents，Pull requests，Issues を read and write，Actions と Metadata を read にし，有効期限は 90 日などにする．
2. エージェント用の設定ディレクトリに保存する．`--insecure-storage` を付けると，トークンはシステムのキーリングではなくそのディレクトリの `hosts.yml` に書き込まれる．キーリングはサンドボックスから届かず，ユーザー自身のトークンも保持している:

    ```bash
    GH_CONFIG_DIR="$HOME/.config/agent-harness/gh" gh auth login --with-token --insecure-storage < token.txt
    ```

3. オプションを有効にし，Home Manager の設定を switch する:

    ```nix
    programs.agent-harness.github.agentToken.enable = true;
    # configDir の既定値は "${config.home.homeDirectory}/.config/agent-harness/gh"．
    ```

生成される Claude の設定は，`env.GH_CONFIG_DIR` に絶対パスの `configDir` を設定し，`github.com` と `api.github.com` を `sandbox.network.allowedDomains` に加え，gh が TLS を検証できるよう `sandbox.enableWeakerNetworkIsolation` を有効にし，ソース設定が列挙していても `gh *` を `sandbox.excludedCommands` から除く．この除去は，ソース設定を正とする原則の唯一の例外である（[ADR-0020](docs/adr/0020_keep_git_push_outside_the_claude_sandbox.md)）．`git push *` を含む他の除外は，ソースの列挙どおりに残る．コミット署名はサンドボックスの外でしか動かないため，`git add x && git commit -m y && git push` のような連結をサンドボックスの外で実行できるよう，`git add *`，`git status *`，`git commit *`，`git push *` をすべて除外しておく．除外された `git push` も `GH_CONFIG_DIR` を引き継ぐので，エージェント用トークンで認証する．`configDir` が相対パスの場合や，`Read(...)` の拒否，`sandbox.filesystem.denyRead` の項目，シークレットパスポリシーのルールがそれを隠す場合は評価が失敗する．ユーザー自身の `~/.config/gh/hosts.yml` の拒否はそのまま残る．オプションが無効なら，生成物は変わらない．

脅威モデル:

- サンドボックス内で動くものはすべて，API の直接呼び出しを含め，権限の範囲内でトークンを使える．安全境界はコマンドルールではなく，トークンの fine-grained な権限である．
- gh は意図してサンドボックス内で実行する．外で実行すると，`gh gist create`，`gh release upload`，`gh api --input` などで読めるローカルファイルを何でもアップロードできてしまう．サンドボックスの外の git push が送れるのはリポジトリのコミットだけである．
- git は `gh auth git-credential`（`gh auth setup-git` で設定する）で認証するため，リモートは HTTPS でなければならない．SSH のリモートはこの仕組みの外で，引き続きユーザーの SSH 鍵を使う．
- 対象は Claude Code だけである．Codex，Devin，Hermes，pi は既定の挙動のままである．

#### GitHub App をトークンの供給元にする

personal access token は期限が切れるたびに作り直す必要がある．`source = "app"` にすると，GitHub App がエージェント用トークンを供給する．ユーザーごとの定期ジョブが，ログイン時と 50 分ごとに有効期間 1 時間のインストールアクセストークンを発行し，`configDir` の `hosts.yml` へ mode 0600 でアトミックに書き込む．ジョブは macOS では launchd エージェントで，スリープ復帰後に逃した実行を 1 回行う．Linux では systemd のユーザーサービスと永続タイマーである．このオプションが Claude の設定に加える変更は，どちらの供給元でも同じである．

1. Webhook を無効にして [GitHub App を登録](https://github.com/settings/apps/new)し，リポジトリ権限を次のようにする: Contents，Pull requests，Issues は read and write，Actions と Metadata は read．Workflows は付与しない．付与すると，サンドボックス内のコードがリポジトリのシークレット付きで動くワークフローを書き換えられてしまう．
2. エージェントが変更してよいリポジトリに App をインストールする．
3. 秘密鍵を生成し，`configDir` の外に mode 0600 で保存する:

    ```bash
    install -m 0600 ~/Downloads/my-agent-app.*.private-key.pem ~/.config/agent-harness/github_app.pem
    ```

4. 供給元を設定して switch する:

    ```nix
    programs.agent-harness.github.agentToken = {
      enable = true;
      source = "app";
      app = {
        id = 123456;
        privateKeyPath = "${config.home.homeDirectory}/.config/agent-harness/github_app.pem";
        # installationId = 7890123;  # App のインストールが複数あるときだけ必要．
      };
    };
    ```

秘密鍵のパスは絶対パスでなければならない．生成される Claude の設定は `permissions.deny` に `Read(//<path>)` を，`sandbox.filesystem.denyRead` にそのパスを加えるので，Claude Code のツールからもサンドボックス内のコマンドからも鍵は読めない．`installationId` を指定しない場合，ジョブは App の唯一のインストールを使い，インストールが複数あると失敗する．

ジョブは更新のたびに，トークンの隣の `agent_harness_token.json` に結果を記録する．ジョブが止まると 1 時間以内に push が失敗し始めるので，次のコマンドで確認する:

```bash
agent-harness github-app-token status --config-dir ~/.config/agent-harness/gh
```

このコマンドは有効期限を表示し，トークンがない場合，最後の更新が失敗した場合，残りが 5 分以下の場合は終了ステータス 1 で終わる．ジョブの出力は macOS では `~/Library/Logs/agent-harness-github-app-token.log` に，Linux ではユーザージャーナルに出る．`agent-harness github-app-token refresh` を同じ引数で実行すれば，任意のタイミングでトークンを発行できる．

App を使う場合に追加される脅威モデル:

- 秘密鍵は SSH 鍵に相当する長期間有効なシークレットで，持っていればすべてのインストールに対してトークンを発行できる．mode 0600 を保ち，他人が読めるバックアップに含めない．
- インストールトークンは 1 時間以内に失効するので，漏れても有効な期間は短い．
- push，プルリクエスト，コメントは App のボットアカウントとして記録される．
- Pull requests の write 権限を持つトークンはプルリクエストを承認できる．ブランチ保護で承認を必須にする場合は，ボットを承認者として数えない．

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

flake は現在 `aarch64-darwin` 向けにだけパッケージを公開している．

```bash
nix run github:furedea/agent-harness -- install --prefix "$HOME"
nix run github:furedea/agent-harness -- verify --prefix "$HOME"
```

### Home Manager

エージェントの設定を Nix で管理している場合は Home Manager モジュールを使う．モジュールは `homeManagerModules.default` として公開しており，現在は `aarch64-darwin` を対象にしている．

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
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

[Nix で合成する](#nix-で合成する) で示したとおり，追加のスキルディレクトリや外部のフックバンドルは，このリポジトリにコピーせずに両方のプロバイダへ合成できる．スキルディレクトリには `SKILL.md` が必要である．インストール名には小文字の ASCII 英字，数字，ハイフンだけが使え，組み込みのスキルを上書きする名前は使えない．通常のファイルはそのままコピーされ，シンボリックリンクは無視される．

各バンドルには `{"version":1}` を持つ `hook_bundle.json` が必要で，Claude の設定，Codex のフック，Codex の feature フラグ，プロバイダのフックスクリプトを含められる．スクリプトは `~/.claude/hooks/external/<name>/` または `~/.codex/hooks/external/<name>/` の下にインストールされる．フックの JSON は構造的にマージされ，外部の Codex 設定が追加できるのは `[features]` のエントリだけである．

モジュールのインターフェースの全体は次のとおり．

| オプション           | デフォルト             | 意味                                                                       |
| -------------------- | ---------------------- | -------------------------------------------------------------------------- |
| `enable`             | `false`                | パッケージと管理対象ファイルを有効にする．                                 |
| `package`            | flake のパッケージ     | `agent-harness` の実行ファイルを選ぶ．                                     |
| `source`             | この flake             | プロファイル集，または完全でフラットなプロファイルのソースツリー．         |
| `profile`            | `"minimal"`            | 同梱の合成のベースを選ぶ．                                                 |
| `agentsMd`           | プロファイルのファイル | 共有の `AGENTS.md` / `CLAUDE.md` のソースを置き換える．                    |
| `commandPermissions` | プロファイルのファイル | プロバイダ中立な `allow` / `ask` / `deny` ルールを置き換える．             |
| `skills`             | `{}`                   | スキルのディレクトリや derivation をインストール名で追加する．             |
| `hooks`              | `{}`                   | バージョン付きフックバンドルのディレクトリや derivation を名前で追加する． |
| `claude.enable`      | `true`                 | Claude Code のファイルをインストールする．                                 |
| `claude.settings`    | `{}`                   | 選んだ Claude のベース設定のキーを任意で上書きする．                       |
| `codex.enable`       | `true`                 | Codex のファイルをインストールする．                                       |
| `codex.settings`     | `{}`                   | 選んだ Codex のベース設定のキーを任意で上書きする．                        |
| `devin.enable`       | `true`                 | Devin のファイルをインストールする．                                       |
| `hermes.enable`      | `true`                 | Hermes Agent のファイルをインストールする．                                |
| `pi.enable`          | `true`                 | pi のファイルをインストールする．                                          |

生成されたフック配線，Bash のコマンド権限，保護パスは，プロバイダ設定のマージ後も harness の管理下に残る．これにより，設定のオーバーレイが強制力を黙って取り除くことを防ぐ．保護パスは，harness が Claude Code，Codex，Devin，Hermes，pi のために生成または同期するすべてのファイル（可変のプロバイダ設定を含む）を対象にし，スキルは対象にしない．プロバイダの `enable` を `false` にすると，そのプロバイダのファイルはインストールされない．

Claude Code が `~/.claude/settings.json` を更新することがあるため，Home Manager モジュールはこのファイルを書き込み可能な通常のファイルとして実体化する．activation のたびに，生成されたトップレベルのキーが既存の対応する値を置き換え，生成された設定にないトップレベルのキーは残る．その他の Claude Code のファイルは，Home Manager が管理するシンボリックリンクのままである．activation では，[インストールされるもの](#インストールされるもの) で説明する Codex，Devin，Hermes Agent の設定の同期も行う．

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

`agent-harness install --prefix "$HOME"` は，生成した harness を Claude Code，Codex，Devin，Hermes Agent，pi の設定ディレクトリに書き込む．

| パス                                     | 用途                                             |
| ---------------------------------------- | ------------------------------------------------ |
| `~/.codex/AGENTS.md`                     | Codex のエージェント指示                         |
| `~/.codex/config.toml`                   | 管理対象の Codex 設定                            |
| `~/.codex/hooks.json`                    | Codex のフック配線                               |
| `~/.codex/hooks/`                        | Codex のフックアダプタ                           |
| `~/.codex/rules/default.rules`           | Codex のコマンド権限                             |
| `~/.codex/skills/`                       | 生成した Codex のスキル                          |
| `~/.claude/CLAUDE.md`                    | Claude Code のエージェント指示                   |
| `~/.claude/settings.json`                | Claude Code の設定                               |
| `~/.claude/hooks/`                       | Claude Code のフックとポリシーガード             |
| `~/.claude/skills/`                      | 生成した Claude Code のスキル                    |
| `~/.claude/statusline/`                  | Claude Code のステータスラインコマンド           |
| `~/.devin/hooks/`                        | Devin のフックアダプタ                           |
| `~/.config/devin/config.json`            | Devin のフックとコマンド権限                     |
| `~/.hermes/hooks.json`                   | Hermes Agent のフックマニフェスト                |
| `~/.hermes/hooks/`                       | Hermes Agent のフックアダプタ                    |
| `~/.hermes/plugins/agent-harness-hooks/` | マニフェストを実行する Hermes Agent のプラグイン |
| `~/.hermes/managed-config.yaml`          | `config.yaml` に同期する Hermes Agent のキー     |
| `~/.pi/agent/hooks.json`                 | pi のフックマニフェスト                          |
| `~/.pi/agent/extensions/hook_bridge.ts`  | マニフェストを実行する pi の拡張                 |
| `~/.pi/hooks/`                           | pi のフックアダプタ                              |

インストールは，管理対象のフックとスキルのディレクトリを置き換え，Claude Code の管理対象の設定を書き直す．Codex の設定の同期は次の管理対象トップレベルキーだけを置き換え，プロジェクトの信頼情報やマーケットプレイスのデータなど，Codex やユーザーが所有する他の状態は残す．

```text
model, model_reasoning_effort, personality, approval_policy, sandbox_mode,
approvals_reviewer, notice, tui, plugins, features, default_permissions,
permissions
```

生成される Claude の設定は，ソースの `claude/settings.json` が宣言する `permissions.allow`，`permissions.ask`，`permissions.deny` のエントリをすべて残し，その後に生成した `Bash(...)`，`Read(...)`，`Edit(...)` のエントリを続ける．各ルールは 1 回だけ現れ，複数のリストで宣言されたルールは最も厳しいリスト（`deny` > `ask` > `allow`）にだけ残る．そのため，マージによって権限が広がることはない．`defaultMode` などの他の権限キーはそのまま引き継がれる．Claude の設定の同期は生成された `permissions` キー全体を置き換えるため，Claude Code が対話的に `~/.claude/settings.json` に保存した承認は次の同期で残らない．永続させたいルールは `command_permissions.json` かソースの `claude/settings.json` に書くこと．

Devin の設定の同期は管理対象のトップレベルキーを置き換え，`permissions` の中では `Exec(...)` のエントリだけを置き換える．`Read(...)`，`Write(...)`，`Fetch(...)`，`mcp__*` などの Exec 以外のルールはユーザーの所有のままである．対話的に `Exec(...)` として保存した承認は次の同期で置き換わるので，永続させたいコマンドルールは `command_permissions.json` に書くこと．

`install` は `~/.hermes/config.yaml` を編集しない．`sync-hermes-config` が `~/.hermes/managed-config.yaml` をそこへマージする．生成された `plugins.enabled` のエントリは重複なく追加され，Hermes Agent の他の状態は残る．Home Manager モジュールは activation でこの同期を実行する．

`verify` は，解決したソースの manifest が宣言する必要なパスと実行時コマンドを確認する．インストールされたファイルの内容を選んだソースと比較はしない．

## 使い方

ほとんどの利用者に必要なのは次だけである．

```bash
agent-harness install --prefix "$HOME"
agent-harness verify --prefix "$HOME"
```

プロジェクトローカルにインストールする場合は，出力先のプレフィックスと実行時ルートの両方を指定する．`--prefix` はファイルを書き込む場所を，`--runtime-root` は生成したフックのコマンドと保護ルールが実行時にファイルを期待する場所を決める．実行時ルートは絶対パスでなければならない．

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
| `generate-claude-settings`       | Claude Code の設定 JSON 全体                     |
| `generate-claude-hooks`          | Claude Code のフック JSON                        |
| `generate-codex-config-source`   | 管理対象の Codex 設定ソース全体                  |
| `generate-codex-config-fragment` | ガード付きファイルシステムの TOML 断片           |
| `generate-codex-hooks`           | Codex のフック JSON                              |
| `generate-codex-rules`           | Codex の execpolicy ルール                       |
| `generate-command-permissions`   | 共有の実行時コマンド権限                         |
| `generate-devin-hooks`           | Devin の設定断片（フックと権限）                 |
| `generate-forbidden-commands`    | グローバルで厳密な禁止コマンドの正規表現ルール   |
| `generate-hermes-hooks`          | Hermes Agent のフックマニフェスト                |
| `generate-hook-bundle`           | 隔離して作るバージョン付きの外部フックバンドル   |
| `generate-pi-hooks`              | pi のフックマニフェスト                          |
| `generate-skills`                | プロバイダ別の組み込みスキルと外部スキルのツリー |
| `sync-claude-settings`           | 既存の Claude 設定へのトップレベルのマージ       |
| `sync-codex-config`              | 既存の Codex 設定への管理対象キーのマージ        |
| `sync-devin-config`              | 既存の Devin 設定への管理対象のマージ            |
| `sync-hermes-config`             | 既存の Hermes 設定への管理対象キーのマージ       |

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

agent-harness generate-codex-hooks \
  --output "$HOME/.codex/hooks.json"

agent-harness generate-codex-rules \
  --output "$HOME/.codex/rules/default.rules"

agent-harness generate-command-permissions \
  --output "$HOME/.claude/hooks/rules/command_permissions.json"

agent-harness generate-forbidden-commands \
  --output "$HOME/.claude/hooks/rules/forbidden_commands.json"

agent-harness generate-codex-config-source \
  --output /tmp/codex-config-source.toml

agent-harness sync-codex-config \
  --source /tmp/codex-config-source.toml \
  --target "$HOME/.codex/config.toml"

agent-harness generate-claude-settings \
  --output "$HOME/.claude/settings.json"
```

フックバンドルの spec には，絶対パスのインストーラ実行ファイルとその引数を並べる．インストーラは隔離した一時ホームで実行され，対応する Claude と Codex のフック成果物だけが取り込まれる．任意のコマンド置換を使うと，ビルド時の実行ファイルのパスを安定した実行時のパスに変えられる．

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

| 目的                                       | 編集するもの                                           |
| ------------------------------------------ | ------------------------------------------------------ |
| 実行時コマンドの要件を宣言する             | `agents/manifest.json`                                 |
| 共有のエージェント指示を変える             | `agents/AGENTS.md`                                     |
| スキルを追加・編集する                     | `agents/skills/<skill>/SKILL.md`                       |
| プロバイダ別のスキルのメタデータを変える   | `agents/skill_rendering.json`                          |
| 共有のコマンド権限を変える                 | `agents/command_permissions.json`                      |
| グローバルで厳密なコマンド形式を変える     | `agents/hooks/rules/{allowed,forbidden}_commands.json` |
| フック配線を追加・変更する                 | `agents/hooks.json`                                    |
| Claude のフックを追加・変更する            | `agents/hooks/*.sh`                                    |
| Codex のフックアダプタを追加・変更する     | `agents/codex/hooks/*.sh`                              |
| Devin，Hermes，pi のフックを追加・変更する | `agents/{devin,hermes,pi}/hooks/`                      |
| Codex のベース設定を変える                 | `agents/codex/config.toml`                             |
| Claude のベース設定を変える                | `agents/claude/settings.json`                          |

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

ジェネレータはこれらのルールを，Claude Code の `allow`，`ask`，`deny` の Bash 権限と，Codex の `allow`，`prompt`，`forbidden` の execpolicy の判定に対応させる．`hooks/rules/` の下の JSON ファイルには，グローバルで厳密なコマンド形式を表す POSIX 拡張正規表現を書く．

リポジトリは，agent-harness を変えずに，次のどちらかの任意のファイルを作って厳密なルールを追加できる．

```text
<git-root>/.agents/hooks/rules/allowed_commands.json
<git-root>/.agents/hooks/rules/forbidden_commands.json
```

どちらのファイルも次のスキーマを使う．

```json
{
    "version": 1,
    "rules": [
        {
            "patterns": ["^uv run --frozen example$"],
            "justification": "Allow the repository's validated example task."
        }
    ]
}
```

プロジェクトの allow ルールが承認できるのは，有効なプロファイルの `command_permissions.json` ですでに宣言された allow のプレフィックスの範囲内にある厳密な形式だけで，新しい共有プレフィックスは導入できない．プロジェクトの forbidden ルールは，一致するコマンドの断片をどれでも拒否できる．forbidden の一致は allow の一致より優先され，不正なプロジェクトのルールファイルは安全側に倒れて拒否する．

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

ソースのルートを解決した後，CLI は `profiles/<profile>` があればそれを選ぶ．なければ，ルートを `manifest.json`，`AGENTS.md`，`hooks/`，`skills/`，`claude/`，`codex/` を含む完全でフラットなプロファイルとして扱う．これにより，dotfiles リポジトリが `agents/` を直接公開でき，同じバイナリがリリースの tarball，Nix のビルド，Cargo のインストール，ローカルのチェックアウトのどれでも動き続ける．

## コントリビュート

開発環境，ローカルのチェックアウトでのコマンド，品質ゲート，リリースの手順は [CONTRIBUTING.md](CONTRIBUTING.md) を参照．
