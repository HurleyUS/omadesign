/// Format the public release version.
/// Converts Cargo's dated nightly counter to four digits; other versions pass through.
/// Args: version from Cargo. Returns the version shown in tags, packages and the app.
pub fn public_version(version: &str) -> String {
    if let Some((base, nightly)) = version.split_once("-nightly.")
        && let Some((date, counter)) = nightly.split_once('.')
        && date.len() == 8
        && date.bytes().all(|b| b.is_ascii_digit())
        && let Ok(counter @ 1..=9999) = counter.parse::<u16>()
    {
        return format!("{base}-nightly.{date}.{counter:04}");
    }
    version.into()
}

/// Normalize a public version for comparison.
/// Cargo and SemVer reject leading zeroes in numeric prerelease fields.
/// Args: public version. Returns the equivalent Cargo version for dated nightlies.
pub fn cargo_version(version: &str) -> String {
    if let Some((base, nightly)) = version.split_once("-nightly.")
        && let Some((date, counter)) = nightly.split_once('.')
        && date.len() == 8
        && date.bytes().all(|b| b.is_ascii_digit())
        && counter.len() == 4
        && counter.bytes().all(|b| b.is_ascii_digit())
        && let Ok(counter @ 1..=9999) = counter.parse::<u16>()
    {
        return format!("{base}-nightly.{date}.{counter}");
    }
    version.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dated_nightlies_round_trip_without_changing_stable_or_legacy_tags() {
        for version in [
            "0.6.3",
            "0.5.5-nightly.2",
            "0.6.3-nightly.20261011.1",
            "0.6.3-nightly.20261011.9999",
        ] {
            assert_eq!(cargo_version(&public_version(version)), version);
        }
        assert_eq!(
            public_version("0.6.3-nightly.20261011.1"),
            "0.6.3-nightly.20261011.0001"
        );
        assert_eq!(
            cargo_version("0.6.3-nightly.20261011.0000"),
            "0.6.3-nightly.20261011.0000"
        );
    }
}
