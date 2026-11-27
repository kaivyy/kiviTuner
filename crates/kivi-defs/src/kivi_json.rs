use crate::model::{Definitions, MapDef};
use serde_json::Error;

pub fn to_json_string(defs: &Definitions) -> Result<String, Error> {
    serde_json::to_string(defs)
}

pub fn to_json_pretty(defs: &Definitions) -> Result<String, Error> {
    serde_json::to_string_pretty(defs)
}

pub fn from_json_str(json_str: &str) -> Result<Definitions, Error> {
    let trimmed = json_str.trim();
    if trimmed.starts_with('[') {
        let maps: Vec<MapDef> = serde_json::from_str(trimmed)?;
        Ok(Definitions {
            maps,
            constants: Vec::new(),
            flags: Vec::new(),
            patches: Vec::new(),
            checksums: Vec::new(),
        })
    } else {
        serde_json::from_str(trimmed)
    }
}

pub fn maps_to_json(maps: &[MapDef]) -> Result<String, Error> {
    serde_json::to_string_pretty(maps)
}

pub fn maps_from_json(json_str: &str) -> Result<Vec<MapDef>, Error> {
    let trimmed = json_str.trim();
    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
    } else {
        let defs: Definitions = serde_json::from_str(trimmed)?;
        Ok(defs.maps)
    }
}
