#[cfg(test)]
mod tests {
    use crate::provider::spec::ProviderId;

    #[test]
    fn hermes_assets_declare_the_manifest_and_register_entrypoint() {
        let assets = ProviderId::Hermes.spec().hook.bridge.assets;
        assert!(
            assets
                .iter()
                .any(|asset| asset.destination.ends_with("/plugin.yaml")
                    && asset.content.contains("name: agent-harness-hooks"))
        );
        assert!(
            assets
                .iter()
                .any(|asset| asset.destination.ends_with("/__init__.py")
                    && asset.content.contains("def register(ctx"))
        );
    }

    #[test]
    fn pi_bridge_runs_the_adapter_with_an_isolated_interpreter() {
        let asset = ProviderId::Pi
            .spec()
            .hook
            .bridge
            .assets
            .iter()
            .find(|asset| asset.destination == ".pi/agent/extensions/hook_bridge.ts")
            .unwrap();
        assert!(
            asset
                .content
                .contains("export default function (pi: ExtensionAPI)")
        );
        assert!(asset.content.contains("pi.on(\"tool_call\""));
        assert!(
            asset
                .content
                .contains(r#"spawn("python3", ["-I", "-B", ADAPTER, "dispatch", event]"#)
        );
        assert!(!asset.content.contains(r#"spawn("python3", [ADAPTER"#));
    }
}
