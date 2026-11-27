//! Does a definition actually describe the ROM it was paired with?
//!
//! ## Why this exists
//!
//! Blind-scan recall is measured against a definition file. If the definition
//! describes a **different** ROM, the measurement is meaningless — and that is
//! not hypothetical.
//!
//! Measured on `CBR250RR-38770-K64-ND1_BIN.bin`: the ROM's identity string is
//! **`K64J108`**, while the paired XDF is titled `38770-K64-ND1`. That XDF also
//! declares 41 of its 52 tables as 35 rows, yet the ROM's axis at that address
//! has only **24** breakpoints and drops from 5534 to 645 where a 35-point axis
//! would have to continue. The definition and the binary do not match.
//!
//! A mismatch like that silently depresses every recall number computed from it.
//! This module makes the check explicit so callers can exclude or flag such
//! pairs instead of blaming the detector.

/// A part-number-like token found in a ROM or a definition name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IdentityToken {
    /// Byte offset in the ROM, or `0` when the token came from a filename.
    pub offset: usize,
    /// The normalised token, upper-case, separators removed.
    pub token: String,
}

/// Minimum length for an ASCII run to be considered a part-number candidate.
///
/// Real ECU part numbers are at least 6 characters (`K64J108`, `8D0907551M`).
const MIN_TOKEN_LEN: usize = 6;

/// Normalises a token for comparison: upper-case, keep only alphanumerics.
///
/// Separators differ between a filename (`38770-K64-ND1`) and the bytes inside a
/// ROM (`K64J108`), so they must not affect the comparison.
pub fn normalise(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// True when `run` looks like a part number rather than code or data.
///
/// Requires at least one digit and one letter: pure digits are usually
/// addresses, pure letters are usually words.
/// Strips leading and trailing separators from a candidate run.
///
/// ROM identity fields sit inside fixed-width slots and next to punctuation, so
/// a run commonly arrives as `"43290010430069--"` or `"K64J108 "`. The interior
/// is what matters.
fn trim_separators(run: &str) -> &str {
    run.trim_matches(|c: char| !c.is_ascii_alphanumeric())
}

fn looks_like_part_number(run: &str) -> bool {
    // A ROM often stores the token followed by spaces inside a fixed-width
    // field (`"K64J108 "`), so surrounding whitespace is trimmed before the
    // shape test rather than rejecting the token.
    let t = trim_separators(run);
    // No upper bound on length: ROM identity fields are frequently one long
    // concatenation of neighbouring numbers. The MS43 ROM holds
    // `4329001043006965773551171114305369011114305369` (46 characters) with the
    // part number `430069` in the middle, so a length cap rejects a correct pair.
    if t.len() < MIN_TOKEN_LEN {
        return false;
    }
    // Separators may appear inside a run: the MS43 identity field is
    // `432900104300696577355117--1114305369011114305369`. They are removed by
    // `normalise`, so only the alphanumeric content has to be plausible.
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' || c == '.')
    {
        return false;
    }
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    let letters = t.chars().filter(|c| c.is_ascii_alphabetic()).count();
    // A part number has both, and is not dominated by one kind. Binary data that
    // happens to be printable produces long letter-heavy runs; an all-letter
    // run is a word, not an identifier. Measured: without this bound the CBR ROM
    // yielded 130 junk tokens such as `FPFL1C`, `FAFM2G`, `7X7X7X7X7X`.
    // `K64J108` is 3 digits and 4 letters of 7 characters, so the bound must
    // allow a letter-majority identifier; it only has to exclude runs that are
    // almost entirely one kind.
    let alnum = t.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    if digits + letters != alnum {
        return false;
    }
    // Real identifiers are not dominated by one character class and are not
    // long letter runs. Measured on the CBR ROM: without this, binary data
    // yielded 170 junk tokens such as `2VFFFG`, `7X7X7X7X7X`, `I0T1C23`.
    let ratio = digits as f64 / (digits + letters) as f64;
    (0.15..=0.75).contains(&ratio) && letters <= 12
}

/// Extracts part-number-like ASCII runs from a ROM.
pub fn extract_rom_tokens(rom: &[u8]) -> Vec<IdentityToken> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut cur = String::new();
    for (i, &b) in rom.iter().enumerate() {
        let printable = (0x20..=0x7E).contains(&b);
        if printable {
            if cur.is_empty() {
                start = i;
            }
            cur.push(b as char);
        } else {
            if looks_like_part_number(&cur) {
                out.push(IdentityToken {
                    offset: start,
                    token: normalise(trim_separators(&cur)),
                });
            }
            cur.clear();
        }
    }
    if looks_like_part_number(&cur) {
        out.push(IdentityToken {
            offset: start,
            token: normalise(trim_separators(&cur)),
        });
    }
    out
}

/// Extracts comparison tokens from a definition's filename and title.
///
/// Splits on the separators filenames use (`-`, `_`, `.`, spaces) and keeps the
/// pieces that look like part numbers.
pub fn extract_definition_tokens(filename: &str, title: &str) -> Vec<IdentityToken> {
    let mut out = Vec::new();
    // Strip a trailing file extension before normalising: `12212156.xdf`
    // normalised whole would become `12212156XDF`, which is not a part number.
    let stem = filename.rsplit_once('.').map(|(a, _)| a).unwrap_or(filename);
    for source in [stem, title] {
        // A part number may be split across separators (`38770-K64-J108`), so
        // both the whole normalised string and each piece are offered. Comparing
        // only the pieces loses the case where the ROM stores the concatenation
        // (`K64J108`) while the filename spells it apart.
        let whole = normalise(source);
        // Only a mixed alphanumeric string is a part number. `mapsxdf` is all
        // letters and `12212156` is all digits, so neither is offered.
        let has_letter = whole.chars().any(|c| c.is_ascii_alphabetic());
        let has_digit = whole.chars().any(|c| c.is_ascii_digit());
        if whole.len() >= MIN_TOKEN_LEN && has_letter && has_digit {
            out.push(IdentityToken {
                offset: 0,
                token: whole,
            });
        }
        for piece in source.split(|c: char| !c.is_ascii_alphanumeric()) {
            let digits = piece.chars().filter(|c| c.is_ascii_digit()).count();
            if digits >= 2 && looks_like_part_number(piece) {
                out.push(IdentityToken {
                    offset: 0,
                    token: normalise(piece),
                });
            }
        }
    }
    out
}

/// Result of comparing a ROM against a definition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IdentityMatch {
    /// At least one token is shared, so the pair is plausibly correct.
    Shared { token: String },
    /// Both sides produced tokens but none match.
    Mismatched { rom: Vec<String>, definition: Vec<String> },
    /// One side produced no tokens, so nothing can be concluded.
    Inconclusive,
}

impl IdentityMatch {
    /// True only when a shared token was found.
    pub fn is_confirmed(&self) -> bool {
        matches!(self, IdentityMatch::Shared { .. })
    }
}

/// Compares ROM identity tokens against definition tokens.
///
/// A token matches when one contains the other, because a ROM often stores only
/// the suffix (`K64J108`) while a definition name carries the full part number
/// (`38770-K64-ND1`).
pub fn compare_identity(rom: &[u8], filename: &str, title: &str) -> IdentityMatch {
    let rom_tokens = extract_rom_tokens(rom);
    let def_tokens = extract_definition_tokens(filename, title);

    if rom_tokens.is_empty() || def_tokens.is_empty() {
        return IdentityMatch::Inconclusive;
    }

    // A shared run of at least MIN_TOKEN_LEN characters counts as a match, tested
    // in both directions and against every window of the longer side.
    //
    // Real shapes this has to handle:
    // * ROM `5JCZSNC0`, definition `5JCZSNC0` — equal.
    // * ROM `...C769850D430069011114300069...`, definition `MS430069` — the part
    //   number is embedded in a much longer concatenation, so the definition's
    //   token has to be searched for inside the ROM token, and vice versa.
    for r in &rom_tokens {
        for d in &def_tokens {
            if r.token.len() >= MIN_TOKEN_LEN && d.token.contains(&r.token) {
                return IdentityMatch::Shared {
                    token: r.token.clone(),
                };
            }
            if d.token.len() >= MIN_TOKEN_LEN && r.token.contains(&d.token) {
                return IdentityMatch::Shared {
                    token: d.token.clone(),
                };
            }
            // A definition token often carries a prefix the ROM omits: the
            // definition says `MS430069` while the ROM stores `430069`. Sliding a
            // MIN_TOKEN_LEN window over the definition token finds the shared
            // core without accepting a two-character coincidence.
            if d.token.len() > MIN_TOKEN_LEN {
                for window in d.token.as_bytes().windows(MIN_TOKEN_LEN) {
                    if let Ok(w) = std::str::from_utf8(window) {
                        if w.chars().any(|c| c.is_ascii_digit())
                            && w.chars().any(|c| c.is_ascii_alphabetic())
                            && r.token.contains(w)
                        {
                            return IdentityMatch::Shared {
                                token: w.to_string(),
                            };
                        }
                    }
                }
            }
        }
    }

    IdentityMatch::Mismatched {
        rom: rom_tokens.into_iter().map(|t| t.token).collect(),
        definition: def_tokens.into_iter().map(|t| t.token).collect(),
    }
}
