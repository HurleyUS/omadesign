//! Document-level linked graphic styles: a named appearance plus the objects
//! that follow it. Redefining the style re-applies it to every member.
use crate::graphic_styles::Appearance;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LinkedStyle {
    pub id: u64,
    pub name: String,
    pub appearance: Appearance,
    #[serde(default)]
    pub members: Vec<u64>,
}

/// Validate saved document styles. Args: style list. Returns the first problem found.
pub(crate) fn validate(styles: &[LinkedStyle]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for style in styles {
        if !ids.insert(style.id) {
            return Err("Linked styles must have unique identities".into());
        }
        if style.name.trim().is_empty() || style.name.chars().count() > 256 {
            return Err("Name linked styles with 1 to 256 characters".into());
        }
        crate::graphic_styles::validate_appearance(&style.appearance)?;
    }
    Ok(())
}

/// Find which style an object follows. Args: styles, object id. Returns the style id.
pub(crate) fn style_of(styles: &[LinkedStyle], object: u64) -> Option<u64> {
    styles.iter().find(|s| s.members.contains(&object)).map(|s| s.id)
}
