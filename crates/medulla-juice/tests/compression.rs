use medulla_juice::{CompressInput, Compressor, Passthrough};

const FIXTURE: &str = include_str!("fixtures/tool-output.json");

#[cfg(feature = "tinyjuice")]
#[test]
fn reformats_json_table_losslessly_and_reports_unicode_token_estimates() {
    let result = medulla_juice::compress(&CompressInput::new(FIXTURE));
    assert!(result.text.len() < FIXTURE.len());
    let before: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    for row in before.as_array().unwrap() {
        for value in row.as_object().unwrap().values() {
            let rendered = match value {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            assert!(
                result.text.contains(&rendered),
                "missing {rendered:?} in {}",
                result.text
            );
        }
    }
    assert_eq!(result.stats.before_bytes, FIXTURE.len());
    assert_eq!(result.stats.after_bytes, result.text.len());
    assert_eq!(
        result.stats.before_tokens,
        FIXTURE.chars().count().div_ceil(4) as u64
    );
    assert_eq!(
        result.stats.after_tokens,
        result.text.chars().count().div_ceil(4) as u64
    );
}

#[cfg(feature = "tinyjuice")]
#[test]
fn preserves_inputs_that_are_not_faithfully_reformatted() {
    let too_many_rows = format!(
        "[{}]",
        (0..45)
            .map(|n| format!("{{\"key\":\"{n}\",\"repeated_long_field_name\":\"value\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for input in [
        "",
        "  \n",
        "{broken",
        "plain text",
        "42",
        too_many_rows.as_str(),
    ] {
        let result = medulla_juice::compress(&CompressInput::new(input));
        assert_eq!(result.text, input);
    }
}

#[cfg(not(feature = "tinyjuice"))]
#[test]
fn facade_is_exact_passthrough_without_tinyjuice() {
    let result = medulla_juice::compress(&CompressInput::new(FIXTURE));
    assert_eq!(result.text, FIXTURE);
    assert_eq!(result.stats.before_bytes, result.stats.after_bytes);
    assert_eq!(result.stats.before_tokens, result.stats.after_tokens);
}

#[test]
fn passthrough_trait_remains_available() {
    let result = Passthrough.compress(&CompressInput::new(FIXTURE));
    assert_eq!(result.text, FIXTURE);
}
