use openhuman_embed::RuntimeConfig;

pub(super) fn apply_compression_policy(config: &mut RuntimeConfig, requested: bool) -> bool {
    let enabled = cfg!(feature = "tinyjuice") && requested;
    config.context.compaction_enabled = enabled;
    enabled
}

#[cfg(test)]
#[path = "compression_tests.rs"]
mod tests;
