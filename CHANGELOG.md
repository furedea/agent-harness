# Changelog

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
