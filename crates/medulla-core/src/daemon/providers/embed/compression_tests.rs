use super::apply_compression_policy;
use openhuman_embed::RuntimeConfig;

#[test]
fn policy_enables_only_when_compiled_and_requested() {
    for requested in [false, true] {
        let mut config = RuntimeConfig::default();
        let actual = apply_compression_policy(&mut config, requested);
        assert_eq!(actual, cfg!(feature = "tinyjuice") && requested);
        assert_eq!(config.context.compaction_enabled, actual);
    }
}
