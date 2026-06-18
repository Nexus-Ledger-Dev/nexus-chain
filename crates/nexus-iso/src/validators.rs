//! Financial identifier validators

use crate::{IsoError, IsoResult};
use regex::Regex;
use std::sync::LazyLock;

// Pre-compiled regexes
static IBAN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Z]{2}[0-9]{2}[A-Z0-9]{4}[0-9]{7}([A-Z0-9]?){0,16}$").unwrap()
});

static BIC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}([A-Z0-9]{3})?$").unwrap()
});

static LEI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Z0-9]{4}00[A-Z0-9]{12}[0-9]{2}$").unwrap()
});

/// Validate IBAN (ISO 13616)
pub fn validate_iban(iban: &str) -> IsoResult<()> {
    let iban = iban.replace([' ', '-'], "").to_uppercase();
    
    // Check format
    if !IBAN_REGEX.is_match(&iban) {
        return Err(IsoError::ValidationFailed("Invalid IBAN format".into()));
    }
    
    // ISO 13616:2020 — complete country/length table.
    let country_lengths: std::collections::HashMap<&str, usize> = [
        ("AD", 24), ("AE", 23), ("AL", 28), ("AT", 20), ("AZ", 28),
        ("BA", 20), ("BE", 16), ("BG", 22), ("BH", 22), ("BI", 27),
        ("BR", 29), ("BY", 28), ("CH", 21), ("CR", 22), ("CY", 28),
        ("CZ", 24), ("DE", 22), ("DJ", 27), ("DK", 18), ("DO", 28),
        ("EE", 20), ("EG", 29), ("ES", 24), ("FI", 18), ("FK", 18),
        ("FO", 18), ("FR", 27), ("GB", 22), ("GE", 22), ("GI", 23),
        ("GL", 18), ("GR", 27), ("GT", 28), ("HR", 21), ("HU", 28),
        ("IE", 22), ("IL", 23), ("IQ", 23), ("IS", 26), ("IT", 27),
        ("JO", 30), ("KW", 30), ("KZ", 20), ("LB", 28), ("LC", 32),
        ("LI", 21), ("LT", 20), ("LU", 20), ("LV", 21), ("LY", 25),
        ("MC", 27), ("MD", 24), ("ME", 22), ("MK", 19), ("MN", 20),
        ("MR", 27), ("MT", 31), ("MU", 30), ("NI", 32), ("NL", 18),
        ("NO", 15), ("OM", 23), ("PK", 24), ("PL", 28), ("PS", 29),
        ("PT", 25), ("QA", 29), ("RO", 24), ("RS", 22), ("RU", 33),
        ("SA", 24), ("SC", 31), ("SD", 18), ("SE", 24), ("SI", 19),
        ("SK", 24), ("SM", 27), ("SO", 23), ("ST", 25), ("SV", 28),
        ("TL", 23), ("TN", 24), ("TR", 26), ("UA", 29), ("VA", 22),
        ("VG", 24), ("XK", 20),
    ].into_iter().collect();
    
    let country = &iban[0..2];
    if let Some(&expected_len) = country_lengths.get(country) {
        if iban.len() != expected_len {
            return Err(IsoError::ValidationFailed(format!(
                "IBAN length mismatch for {}: expected {}, got {}",
                country, expected_len, iban.len()
            )));
        }
    }
    
    // Validate checksum (ISO 7064 mod 97-10)
    let rearranged = format!("{}{}", &iban[4..], &iban[0..4]);
    
    let numeric: String = rearranged.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                c.to_string()
            } else {
                // A=10, B=11, ..., Z=35
                ((c as u8) - b'A' + 10).to_string()
            }
        })
        .collect();
    
    // Calculate mod 97 on the large number
    let mut remainder = 0u64;
    for chunk in numeric.as_bytes().chunks(7) {
        let s = std::str::from_utf8(chunk).unwrap();
        let num: u64 = format!("{}{}", remainder, s).parse().unwrap();
        remainder = num % 97;
    }
    
    if remainder != 1 {
        return Err(IsoError::ValidationFailed("Invalid IBAN checksum".into()));
    }
    
    Ok(())
}

/// Validate BIC/SWIFT code (ISO 9362)
pub fn validate_bic(bic: &str) -> IsoResult<()> {
    let bic = bic.to_uppercase();
    
    if !BIC_REGEX.is_match(&bic) {
        return Err(IsoError::ValidationFailed("Invalid BIC format".into()));
    }
    
    // Check for valid country code (position 5-6)
    let country = &bic[4..6];
    if !is_valid_country_code(country) {
        return Err(IsoError::ValidationFailed(format!(
            "Invalid country code in BIC: {}", country
        )));
    }
    
    Ok(())
}

/// Validate LEI (ISO 17442)
pub fn validate_lei(lei: &str) -> IsoResult<()> {
    let lei = lei.to_uppercase();
    
    if lei.len() != 20 {
        return Err(IsoError::ValidationFailed(format!(
            "LEI must be 20 characters, got {}", lei.len()
        )));
    }
    
    if !LEI_REGEX.is_match(&lei) {
        return Err(IsoError::ValidationFailed("Invalid LEI format".into()));
    }
    
    // Validate checksum (same as IBAN)
    let numeric: String = lei.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                c.to_string()
            } else {
                ((c as u8) - b'A' + 10).to_string()
            }
        })
        .collect();
    
    let mut remainder = 0u64;
    for chunk in numeric.as_bytes().chunks(7) {
        let s = std::str::from_utf8(chunk).unwrap();
        let num: u64 = format!("{}{}", remainder, s).parse().unwrap();
        remainder = num % 97;
    }
    
    if remainder != 1 {
        return Err(IsoError::ValidationFailed("Invalid LEI checksum".into()));
    }
    
    Ok(())
}

/// Validate credit card number (Luhn algorithm)
pub fn validate_card_number(card: &str) -> IsoResult<()> {
    let card: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    
    if card.len() < 13 || card.len() > 19 {
        return Err(IsoError::ValidationFailed(format!(
            "Card number must be 13-19 digits, got {}", card.len()
        )));
    }
    
    // Luhn algorithm
    let mut sum = 0;
    let mut double = false;
    
    for c in card.chars().rev() {
        let mut digit = c.to_digit(10).unwrap();
        
        if double {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        
        sum += digit;
        double = !double;
    }
    
    if sum % 10 != 0 {
        return Err(IsoError::ValidationFailed("Invalid card number checksum".into()));
    }
    
    Ok(())
}

/// Get card network from BIN (first 6-8 digits)
pub fn get_card_network(card: &str) -> Option<&'static str> {
    let card: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    
    if card.len() < 6 {
        return None;
    }
    
    let prefix2: u32 = card[0..2].parse().ok()?;
    let prefix4: u32 = card[0..4].parse().ok()?;
    let _prefix6: u32 = card[0..6].parse().ok()?;
    
    // Visa
    if card.starts_with('4') {
        return Some("Visa");
    }
    
    // Mastercard (51-55, 2221-2720)
    if (51..=55).contains(&prefix2) || (2221..=2720).contains(&prefix4) {
        return Some("Mastercard");
    }
    
    // American Express (34, 37)
    if prefix2 == 34 || prefix2 == 37 {
        return Some("American Express");
    }
    
    // Discover (6011, 65, 644-649)
    if prefix4 == 6011 || prefix2 == 65 || (644..=649).contains(&(card[0..3].parse::<u32>().ok()?)) {
        return Some("Discover");
    }
    
    // JCB (3528-3589)
    if (3528..=3589).contains(&prefix4) {
        return Some("JCB");
    }
    
    // Diners Club
    if (300..=305).contains(&(card[0..3].parse::<u32>().ok()?)) || prefix2 == 36 || prefix2 == 38 {
        return Some("Diners Club");
    }
    
    // UnionPay (62)
    if prefix2 == 62 {
        return Some("UnionPay");
    }
    
    None
}

/// Check if country code is valid (ISO 3166-1 alpha-2)
fn is_valid_country_code(code: &str) -> bool {
    // Simplified - in production use full list
    const VALID_CODES: &[&str] = &[
        "AD", "AE", "AF", "AG", "AL", "AM", "AO", "AR", "AT", "AU",
        "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ",
        "BM", "BN", "BO", "BR", "BS", "BT", "BW", "BY", "BZ", "CA",
        "CD", "CF", "CG", "CH", "CI", "CL", "CM", "CN", "CO", "CR",
        "CU", "CV", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ",
        "EC", "EE", "EG", "ER", "ES", "ET", "FI", "FJ", "FR", "GA",
        "GB", "GD", "GE", "GH", "GM", "GN", "GQ", "GR", "GT", "GW",
        "GY", "HK", "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IN",
        "IQ", "IR", "IS", "IT", "JM", "JO", "JP", "KE", "KG", "KH",
        "KI", "KM", "KN", "KP", "KR", "KW", "KZ", "LA", "LB", "LC",
        "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC",
        "MD", "ME", "MG", "MK", "ML", "MM", "MN", "MO", "MR", "MT",
        "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NE", "NG", "NI",
        "NL", "NO", "NP", "NR", "NZ", "OM", "PA", "PE", "PG", "PH",
        "PK", "PL", "PT", "PW", "PY", "QA", "RO", "RS", "RU", "RW",
        "SA", "SB", "SC", "SD", "SE", "SG", "SI", "SK", "SL", "SM",
        "SN", "SO", "SR", "SS", "ST", "SV", "SY", "SZ", "TD", "TG",
        "TH", "TJ", "TL", "TM", "TN", "TO", "TR", "TT", "TV", "TW",
        "TZ", "UA", "UG", "US", "UY", "UZ", "VA", "VC", "VE", "VN",
        "VU", "WS", "XK", "YE", "ZA", "ZM", "ZW",
    ];
    
    VALID_CODES.contains(&code)
}

/// Validate a business identifier code
pub struct IdentifierValidator;

impl IdentifierValidator {
    /// Validate any identifier type
    pub fn validate(identifier_type: &str, value: &str) -> IsoResult<()> {
        match identifier_type.to_uppercase().as_str() {
            "IBAN" => validate_iban(value),
            "BIC" | "SWIFT" => validate_bic(value),
            "LEI" => validate_lei(value),
            "PAN" | "CARD" => validate_card_number(value),
            _ => Err(IsoError::UnknownMessageType(format!(
                "Unknown identifier type: {}", identifier_type
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_iban() {
        assert!(validate_iban("DE89370400440532013000").is_ok());
        assert!(validate_iban("GB82WEST12345698765432").is_ok());
        assert!(validate_iban("FR7630006000011234567890189").is_ok());
    }
    
    #[test]
    fn test_invalid_iban() {
        assert!(validate_iban("DE89370400440532013001").is_err()); // Bad checksum
        assert!(validate_iban("XX89370400440532013000").is_err()); // Invalid format
    }
    
    #[test]
    fn test_valid_bic() {
        assert!(validate_bic("DEUTDEFF").is_ok());
        assert!(validate_bic("COBADEFFXXX").is_ok());
        assert!(validate_bic("BUKBGB22").is_ok());
    }
    
    #[test]
    fn test_valid_lei() {
        // Example LEI (check digits would need to be valid)
        assert!(validate_lei("529900W18LQJJN6SJ336").is_ok());
    }
    
    #[test]
    fn test_card_validation() {
        assert!(validate_card_number("4111111111111111").is_ok()); // Visa test
        assert!(validate_card_number("5500000000000004").is_ok()); // MC test
        assert!(validate_card_number("4111111111111112").is_err()); // Bad checksum
    }
    
    #[test]
    fn test_card_network_detection() {
        assert_eq!(get_card_network("4111111111111111"), Some("Visa"));
        assert_eq!(get_card_network("5500000000000004"), Some("Mastercard"));
        assert_eq!(get_card_network("378282246310005"), Some("American Express"));
        assert_eq!(get_card_network("6011111111111117"), Some("Discover"));
    }
}
