//! ISO 4217 Currency codes

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Currency information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Currency {
    /// ISO 4217 alphabetic code
    pub code: &'static str,
    /// ISO 4217 numeric code
    pub numeric: &'static str,
    /// Number of decimal places
    pub decimals: u8,
    /// Currency name
    pub name: &'static str,
}

static CURRENCIES: LazyLock<HashMap<&'static str, Currency>> = LazyLock::new(|| {
    let currencies = vec![
        Currency { code: "AED", numeric: "784", decimals: 2, name: "UAE Dirham" },
        Currency { code: "AUD", numeric: "036", decimals: 2, name: "Australian Dollar" },
        Currency { code: "BRL", numeric: "986", decimals: 2, name: "Brazilian Real" },
        Currency { code: "CAD", numeric: "124", decimals: 2, name: "Canadian Dollar" },
        Currency { code: "CHF", numeric: "756", decimals: 2, name: "Swiss Franc" },
        Currency { code: "CNY", numeric: "156", decimals: 2, name: "Chinese Yuan" },
        Currency { code: "EUR", numeric: "978", decimals: 2, name: "Euro" },
        Currency { code: "GBP", numeric: "826", decimals: 2, name: "British Pound" },
        Currency { code: "HKD", numeric: "344", decimals: 2, name: "Hong Kong Dollar" },
        Currency { code: "INR", numeric: "356", decimals: 2, name: "Indian Rupee" },
        Currency { code: "JPY", numeric: "392", decimals: 0, name: "Japanese Yen" },
        Currency { code: "KRW", numeric: "410", decimals: 0, name: "South Korean Won" },
        Currency { code: "MXN", numeric: "484", decimals: 2, name: "Mexican Peso" },
        Currency { code: "NOK", numeric: "578", decimals: 2, name: "Norwegian Krone" },
        Currency { code: "NZD", numeric: "554", decimals: 2, name: "New Zealand Dollar" },
        Currency { code: "PLN", numeric: "985", decimals: 2, name: "Polish Zloty" },
        Currency { code: "RUB", numeric: "643", decimals: 2, name: "Russian Ruble" },
        Currency { code: "SEK", numeric: "752", decimals: 2, name: "Swedish Krona" },
        Currency { code: "SGD", numeric: "702", decimals: 2, name: "Singapore Dollar" },
        Currency { code: "THB", numeric: "764", decimals: 2, name: "Thai Baht" },
        Currency { code: "TRY", numeric: "949", decimals: 2, name: "Turkish Lira" },
        Currency { code: "USD", numeric: "840", decimals: 2, name: "US Dollar" },
        Currency { code: "ZAR", numeric: "710", decimals: 2, name: "South African Rand" },
        // Cryptocurrencies (using X prefix per ISO 4217)
        Currency { code: "XBT", numeric: "000", decimals: 8, name: "Bitcoin" },
        Currency { code: "XET", numeric: "000", decimals: 18, name: "Ethereum" },
    ];
    
    currencies.into_iter()
        .map(|c| (c.code, c))
        .collect()
});

/// Get currency by alphabetic code
pub fn get_currency(code: &str) -> Option<&'static Currency> {
    CURRENCIES.get(code.to_uppercase().as_str())
}

/// Get currency by numeric code
pub fn get_currency_by_numeric(numeric: &str) -> Option<&'static Currency> {
    CURRENCIES.values().find(|c| c.numeric == numeric)
}

/// Validate currency code
pub fn is_valid_currency(code: &str) -> bool {
    CURRENCIES.contains_key(code.to_uppercase().as_str())
}

/// Convert amount from minor to major units
pub fn minor_to_major(amount: i64, currency: &str) -> Option<f64> {
    get_currency(currency).map(|c| {
        amount as f64 / 10_f64.powi(c.decimals as i32)
    })
}

/// Convert amount from major to minor units
pub fn major_to_minor(amount: f64, currency: &str) -> Option<i64> {
    get_currency(currency).map(|c| {
        (amount * 10_f64.powi(c.decimals as i32)).round() as i64
    })
}

/// Format amount with currency
pub fn format_amount(amount: i64, currency: &str) -> Option<String> {
    get_currency(currency).map(|c| {
        let major = amount as f64 / 10_f64.powi(c.decimals as i32);
        format!("{:.prec$} {}", major, c.code, prec = c.decimals as usize)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_currency() {
        let usd = get_currency("USD").unwrap();
        assert_eq!(usd.numeric, "840");
        assert_eq!(usd.decimals, 2);
        
        let jpy = get_currency("JPY").unwrap();
        assert_eq!(jpy.decimals, 0);
    }
    
    #[test]
    fn test_conversions() {
        assert_eq!(minor_to_major(10000, "USD"), Some(100.0));
        assert_eq!(major_to_minor(100.0, "USD"), Some(10000));
        
        assert_eq!(minor_to_major(1000, "JPY"), Some(1000.0));
        assert_eq!(major_to_minor(1000.0, "JPY"), Some(1000));
    }
    
    #[test]
    fn test_format() {
        assert_eq!(format_amount(10050, "USD"), Some("100.50 USD".to_string()));
        assert_eq!(format_amount(1000, "JPY"), Some("1000 JPY".to_string()));
    }
}
