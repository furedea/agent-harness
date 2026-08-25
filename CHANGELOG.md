# Changelog

## [0.7.1](https://github.com/furedea/agent-harness/compare/agent-harness-v0.7.0...agent-harness-v0.7.1) (2026-08-25)


### Bug Fixes

* **release:** use pre-major compatibility bumps ([#108](https://github.com/furedea/agent-harness/issues/108)) ([c3ad0a8](https://github.com/furedea/agent-harness/commit/c3ad0a8227ef9d4bb0192d297f796f40316184f0))

## [0.7.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.6.0...agent-harness-v0.7.0) (2026-08-24)


### ⚠ BREAKING CHANGES

* remove Herdr-specific integration ([#106](https://github.com/furedea/agent-harness/issues/106))

### Features

* support project command policies ([#101](https://github.com/furedea/agent-harness/issues/101)) ([d4f6e83](https://github.com/furedea/agent-harness/commit/d4f6e83f2d710c05a0494b4a34c57e1272ac477e))


### Bug Fixes

* keep breaking releases pre-major ([#107](https://github.com/furedea/agent-harness/issues/107)) ([428ea2f](https://github.com/furedea/agent-harness/commit/428ea2ff1ee19bc2299ae9e00e15805f3089e245))
* protect external hook assets ([#105](https://github.com/furedea/agent-harness/issues/105)) ([7de54cf](https://github.com/furedea/agent-harness/commit/7de54cfc0b3cc931d53996f5709af46a02f228f4))


### Code Refactoring

* remove Herdr-specific integration ([#106](https://github.com/furedea/agent-harness/issues/106)) ([56f0f54](https://github.com/furedea/agent-harness/commit/56f0f54ff935fdd8986553381878c3a45d2d0692))

## [0.6.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.5.0...agent-harness-v0.6.0) (2026-08-20)


### Features

* **cli:** describe top-level commands ([#95](https://github.com/furedea/agent-harness/issues/95)) ([5191269](https://github.com/furedea/agent-harness/commit/5191269971ef74e2b83c830e6266fff303a657ef))
* **cli:** list managed harness components ([#92](https://github.com/furedea/agent-harness/issues/92)) ([28e5096](https://github.com/furedea/agent-harness/commit/28e509637e3d2a6203f2ec99a3ddf7b05827784e))
* compose external hook bundles ([#100](https://github.com/furedea/agent-harness/issues/100)) ([240334a](https://github.com/furedea/agent-harness/commit/240334a210c114edba9fc8bc359b0a3ab102d55e))

## [0.5.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.4.0...agent-harness-v0.5.0) (2026-08-09)


### Features

* **skills:** add external skill sources ([#88](https://github.com/furedea/agent-harness/issues/88)) ([8445c1c](https://github.com/furedea/agent-harness/commit/8445c1cd49625999c19962580bdf7f2626ba8145))


### Bug Fixes

* **deps:** update rust crate anyhow to v1.0.104 ([#86](https://github.com/furedea/agent-harness/issues/86)) ([6f7e089](https://github.com/furedea/agent-harness/commit/6f7e089c63fdca9222741d08bb51824d5fa09646))
* **deps:** update rust crate clap to v4.6.6 ([#87](https://github.com/furedea/agent-harness/issues/87)) ([c4ea6d3](https://github.com/furedea/agent-harness/commit/c4ea6d398ad2e17482e90d9427ff44decd7b8079))
* **deps:** update rust crate toml_edit to v0.25.13 ([#79](https://github.com/furedea/agent-harness/issues/79)) ([b615094](https://github.com/furedea/agent-harness/commit/b615094168ed58821dae0d835dcf280390217b47))

## [0.4.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.3.1...agent-harness-v0.4.0) (2026-07-12)


### Features

* **herdr:** integrate upstream session hooks ([#76](https://github.com/furedea/agent-harness/issues/76)) ([b0ca5e3](https://github.com/furedea/agent-harness/commit/b0ca5e3d3d9a57550f3102371165c0ca8d3419ff))
* **skills:** add herdr skill ([#69](https://github.com/furedea/agent-harness/issues/69)) ([a497000](https://github.com/furedea/agent-harness/commit/a497000d836c08c0d0bcb0d3c6eb6c748dd6781d))


### Bug Fixes

* **deps:** update rust crate serde_json to v1.0.150 ([#66](https://github.com/furedea/agent-harness/issues/66)) ([5a0f9be](https://github.com/furedea/agent-harness/commit/5a0f9beb0e87fcdaf32911346bf53ce54e56da52))
* **hooks:** allow git commit --amend --no-edit ([#67](https://github.com/furedea/agent-harness/issues/67)) ([973b8bd](https://github.com/furedea/agent-harness/commit/973b8bd146d7fed6bfaf06a1266580bdccb11169))

## [0.3.1](https://github.com/furedea/agent-harness/compare/agent-harness-v0.3.0...agent-harness-v0.3.1) (2026-06-01)


### Bug Fixes

* **claude:** install secret path policy with hook rules ([#53](https://github.com/furedea/agent-harness/issues/53)) ([aae3efe](https://github.com/furedea/agent-harness/commit/aae3efef48e4305baec09574f8ff27c7435402e8))

## [0.3.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.2.0...agent-harness-v0.3.0) (2026-06-01)


### Features

* **install:** add release installer script ([#42](https://github.com/furedea/agent-harness/issues/42)) ([7869923](https://github.com/furedea/agent-harness/commit/78699230215437167b75ba51e6f033f3c29306d2))
* **release:** add cargo-dist packaging ([#47](https://github.com/furedea/agent-harness/issues/47)) ([917d4ed](https://github.com/furedea/agent-harness/commit/917d4ed5a611a15327cbba6690a8187985b057f8))
* **workflow:** add PR delivery policy ([#48](https://github.com/furedea/agent-harness/issues/48)) ([4c0a3c8](https://github.com/furedea/agent-harness/commit/4c0a3c8c4f24a475f2408321a7afb74ec31f02ca))
* **workflow:** add worktree branch policy ([#49](https://github.com/furedea/agent-harness/issues/49)) ([576ea6e](https://github.com/furedea/agent-harness/commit/576ea6e4af044f9718e55b2d591cd2870afb2c24))
* **workflow:** document ff-only main updates ([#50](https://github.com/furedea/agent-harness/issues/50)) ([e54410d](https://github.com/furedea/agent-harness/commit/e54410dc32143327ddbdaef0b5969091bebdb12c))


### Bug Fixes

* **deps:** pin dependencies ([#3](https://github.com/furedea/agent-harness/issues/3)) ([b36843c](https://github.com/furedea/agent-harness/commit/b36843cca169e221889df5c258701498e234e06c))

## [0.2.0](https://github.com/furedea/agent-harness/compare/agent-harness-v0.1.0...agent-harness-v0.2.0) (2026-05-28)


### Features

* add agent harness CLI ([#6](https://github.com/furedea/agent-harness/issues/6)) ([7f3c775](https://github.com/furedea/agent-harness/commit/7f3c775b09b891fc00b6cdcbacec9f051f32e1df))
* expose Nix package and Home Manager module ([#7](https://github.com/furedea/agent-harness/issues/7)) ([137a5da](https://github.com/furedea/agent-harness/commit/137a5dae2d20a1378cac8b865405065a60a4f67e))
* generate command policy files ([#14](https://github.com/furedea/agent-harness/issues/14)) ([9a3c7d0](https://github.com/furedea/agent-harness/commit/9a3c7d000801769e3da9f5d07658dee562f0fbf5))
* generate hook configurations ([#17](https://github.com/furedea/agent-harness/issues/17)) ([b5ec4d8](https://github.com/furedea/agent-harness/commit/b5ec4d8068d9f1b2b033e76051460e8fd4cc432a))
* generate protected path policy ([#18](https://github.com/furedea/agent-harness/issues/18)) ([725c04c](https://github.com/furedea/agent-harness/commit/725c04c1ad51c9b7ea1d220b1b6090a7ea6dab08))
* refine agent harness command layout ([#12](https://github.com/furedea/agent-harness/issues/12)) ([ca57421](https://github.com/furedea/agent-harness/commit/ca574217c1e521442588ecc92fc5028e1b8dca25))
* render provider skills ([#20](https://github.com/furedea/agent-harness/issues/20)) ([e7e4a21](https://github.com/furedea/agent-harness/commit/e7e4a21eae8d54d7b0b741b394b01c1823b17458))
* **skills:** add ADR operation skill ([#30](https://github.com/furedea/agent-harness/issues/30)) ([3d89283](https://github.com/furedea/agent-harness/commit/3d89283680059d50ab42ef044d42979eb0c42975))
* **skills:** add git workflow skill ([#28](https://github.com/furedea/agent-harness/issues/28)) ([9764417](https://github.com/furedea/agent-harness/commit/9764417ab059e0f76c530f998861abe5169e2780))
* standardize CLI option names ([#13](https://github.com/furedea/agent-harness/issues/13)) ([110c284](https://github.com/furedea/agent-harness/commit/110c284737f823a910ba86ad5d90c9352a7cddb9))
* synthesize provider configs ([#19](https://github.com/furedea/agent-harness/issues/19)) ([41dfb40](https://github.com/furedea/agent-harness/commit/41dfb40906a7bad6d711188f6021bce1f9074ccc))
* use file-level generated outputs ([#11](https://github.com/furedea/agent-harness/issues/11)) ([64cf683](https://github.com/furedea/agent-harness/commit/64cf683eaec64aa13f94691f115eda8464084fbd))
* use packaged source by default ([#32](https://github.com/furedea/agent-harness/issues/32)) ([6c44f34](https://github.com/furedea/agent-harness/commit/6c44f34014e8bf70fbc11082b68e2b7aa83be67f))


### Bug Fixes

* avoid git in json syntax test ([#26](https://github.com/furedea/agent-harness/issues/26)) ([d64b992](https://github.com/furedea/agent-harness/commit/d64b992c84ed3632ce7f1957b671bb5b64a98e64))
* install generated codex guarded config ([#25](https://github.com/furedea/agent-harness/issues/25)) ([1647802](https://github.com/furedea/agent-harness/commit/16478025e3a0e1133c753f7446cab0b3997981b8))
* keep user tools visible in dev shell ([#23](https://github.com/furedea/agent-harness/issues/23)) ([9a572e6](https://github.com/furedea/agent-harness/commit/9a572e613ca756d9baab05dedc60604399d52357))
* protect installed harness paths only ([#27](https://github.com/furedea/agent-harness/issues/27)) ([9911632](https://github.com/furedea/agent-harness/commit/991163203ab90adee596a444781dd508bcd6608c))
* **release:** use token for release PRs ([#38](https://github.com/furedea/agent-harness/issues/38)) ([31c010e](https://github.com/furedea/agent-harness/commit/31c010e125d13695ea1e2540e434507cf7d5b841))
* validate skill patch descriptions ([#33](https://github.com/furedea/agent-harness/issues/33)) ([519a9ad](https://github.com/furedea/agent-harness/commit/519a9ad7a21669a24fc76821ded39d3ed69eb0e8))

## Changelog
