//! ISO 8583 message handling for card transactions

use crate::{IsoError, IsoResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ISO 8583 message types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// Authorization request
    AuthorizationRequest = 0x0100,
    /// Authorization response
    AuthorizationResponse = 0x0110,
    /// Financial request (purchase)
    FinancialRequest = 0x0200,
    /// Financial response
    FinancialResponse = 0x0210,
    /// Reversal request
    ReversalRequest = 0x0400,
    /// Reversal response
    ReversalResponse = 0x0410,
    /// Network management request
    NetworkManagementRequest = 0x0800,
    /// Network management response
    NetworkManagementResponse = 0x0810,
}

impl MessageType {
    pub fn from_code(code: u16) -> Option<Self> {
        match code {
            0x0100 => Some(Self::AuthorizationRequest),
            0x0110 => Some(Self::AuthorizationResponse),
            0x0200 => Some(Self::FinancialRequest),
            0x0210 => Some(Self::FinancialResponse),
            0x0400 => Some(Self::ReversalRequest),
            0x0410 => Some(Self::ReversalResponse),
            0x0800 => Some(Self::NetworkManagementRequest),
            0x0810 => Some(Self::NetworkManagementResponse),
            _ => None,
        }
    }
    
    pub fn to_code(&self) -> u16 {
        *self as u16
    }
}

/// ISO 8583 data element
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataElement {
    pub id: u8,
    pub value: Vec<u8>,
}

/// ISO 8583 message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Iso8583Message {
    /// Message type indicator
    pub mti: MessageType,
    
    /// Primary bitmap (fields 1-64)
    pub primary_bitmap: u64,
    
    /// Secondary bitmap (fields 65-128)
    pub secondary_bitmap: Option<u64>,
    
    /// Data elements
    pub elements: HashMap<u8, Vec<u8>>,
}

impl Iso8583Message {
    pub fn new(mti: MessageType) -> Self {
        Self {
            mti,
            primary_bitmap: 0,
            secondary_bitmap: None,
            elements: HashMap::new(),
        }
    }
    
    /// Set a data element
    pub fn set_element(&mut self, id: u8, value: Vec<u8>) {
        if id <= 64 {
            self.primary_bitmap |= 1u64 << (64 - id);
        } else {
            if self.secondary_bitmap.is_none() {
                self.secondary_bitmap = Some(0);
                self.primary_bitmap |= 1u64 << 63; // Bit 1 indicates secondary bitmap
            }
            let bitmap = self.secondary_bitmap.as_mut().unwrap();
            *bitmap |= 1u64 << (128 - id);
        }
        self.elements.insert(id, value);
    }
    
    /// Get a data element
    pub fn get_element(&self, id: u8) -> Option<&Vec<u8>> {
        self.elements.get(&id)
    }
    
    /// Get element as string (ASCII)
    pub fn get_element_string(&self, id: u8) -> Option<String> {
        self.elements.get(&id)
            .and_then(|v| String::from_utf8(v.clone()).ok())
    }
    
    // Standard field accessors
    
    /// Field 2: Primary Account Number (PAN)
    pub fn pan(&self) -> Option<String> {
        self.get_element_string(2)
    }
    
    /// Field 3: Processing Code
    pub fn processing_code(&self) -> Option<String> {
        self.get_element_string(3)
    }
    
    /// Field 4: Transaction Amount
    pub fn amount(&self) -> Option<u64> {
        self.get_element_string(4)
            .and_then(|s| s.parse().ok())
    }
    
    /// Field 11: Systems Trace Audit Number (STAN)
    pub fn stan(&self) -> Option<String> {
        self.get_element_string(11)
    }
    
    /// Field 12: Local Transaction Time
    pub fn local_time(&self) -> Option<String> {
        self.get_element_string(12)
    }
    
    /// Field 13: Local Transaction Date
    pub fn local_date(&self) -> Option<String> {
        self.get_element_string(13)
    }
    
    /// Field 14: Expiration Date
    pub fn expiration_date(&self) -> Option<String> {
        self.get_element_string(14)
    }
    
    /// Field 22: Point of Service Entry Mode
    pub fn pos_entry_mode(&self) -> Option<String> {
        self.get_element_string(22)
    }
    
    /// Field 23: Card Sequence Number
    pub fn card_sequence_number(&self) -> Option<String> {
        self.get_element_string(23)
    }
    
    /// Field 32: Acquiring Institution ID
    pub fn acquiring_institution_id(&self) -> Option<String> {
        self.get_element_string(32)
    }
    
    /// Field 35: Track 2 Data
    pub fn track2_data(&self) -> Option<String> {
        self.get_element_string(35)
    }
    
    /// Field 37: Retrieval Reference Number
    pub fn retrieval_reference(&self) -> Option<String> {
        self.get_element_string(37)
    }
    
    /// Field 38: Authorization Code
    pub fn authorization_code(&self) -> Option<String> {
        self.get_element_string(38)
    }
    
    /// Field 39: Response Code
    pub fn response_code(&self) -> Option<String> {
        self.get_element_string(39)
    }
    
    /// Field 41: Card Acceptor Terminal ID
    pub fn terminal_id(&self) -> Option<String> {
        self.get_element_string(41)
    }
    
    /// Field 42: Card Acceptor ID
    pub fn merchant_id(&self) -> Option<String> {
        self.get_element_string(42)
    }
    
    /// Field 43: Card Acceptor Name/Location
    pub fn merchant_name(&self) -> Option<String> {
        self.get_element_string(43)
    }
    
    /// Field 49: Currency Code (Transaction)
    pub fn currency_code(&self) -> Option<String> {
        self.get_element_string(49)
    }
    
    /// Field 55: EMV Data (ICC)
    pub fn emv_data(&self) -> Option<&Vec<u8>> {
        self.get_element(55)
    }
}

// ---------------------------------------------------------------------------
// ISO 8583 field type definitions
// Each field has a fixed or variable length encoding.
// ---------------------------------------------------------------------------

/// Encoding class for an ISO 8583 data element.
#[derive(Clone, Copy, Debug)]
enum FieldEncoding {
    /// Fixed-length field; value is padded/truncated to exactly `len` bytes.
    Fixed(usize),
    /// Variable-length field with a 2-digit ASCII decimal length prefix; max `len`.
    LlVar(usize),
    /// Variable-length field with a 3-digit ASCII decimal length prefix; max `len`.
    LllVar(usize),
}

/// Returns the encoding spec for a given DE field number (1-indexed, 1-128).
fn field_encoding(id: u8) -> Option<FieldEncoding> {
    Some(match id {
        1  => FieldEncoding::Fixed(8),   // Secondary bitmap
        2  => FieldEncoding::LlVar(19),  // PAN
        3  => FieldEncoding::Fixed(6),   // Processing code
        4  => FieldEncoding::Fixed(12),  // Amount
        5  => FieldEncoding::Fixed(12),  // Settlement amount
        6  => FieldEncoding::Fixed(12),  // Cardholder billing amount
        7  => FieldEncoding::Fixed(10),  // Transmission date/time
        8  => FieldEncoding::Fixed(8),   // Cardholder billing fee
        9  => FieldEncoding::Fixed(8),   // Settlement conversion rate
        10 => FieldEncoding::Fixed(8),   // Cardholder conversion rate
        11 => FieldEncoding::Fixed(6),   // STAN
        12 => FieldEncoding::Fixed(6),   // Local time
        13 => FieldEncoding::Fixed(4),   // Local date
        14 => FieldEncoding::Fixed(4),   // Expiry date
        15 => FieldEncoding::Fixed(4),   // Settlement date
        16 => FieldEncoding::Fixed(4),   // Currency conversion date
        17 => FieldEncoding::Fixed(4),   // Capture date
        18 => FieldEncoding::Fixed(4),   // Merchant type (MCC)
        19 => FieldEncoding::Fixed(3),   // Acquiring institution country
        20 => FieldEncoding::Fixed(3),   // PAN country
        21 => FieldEncoding::Fixed(3),   // Forwarding institution country
        22 => FieldEncoding::Fixed(3),   // POS entry mode
        23 => FieldEncoding::Fixed(3),   // Card sequence number
        24 => FieldEncoding::Fixed(3),   // Network identifier
        25 => FieldEncoding::Fixed(2),   // POS condition code
        26 => FieldEncoding::Fixed(2),   // POS capture code
        27 => FieldEncoding::Fixed(1),   // Auth ID response length
        28 => FieldEncoding::Fixed(9),   // Amount, transaction fee
        29 => FieldEncoding::Fixed(9),   // Amount, settlement fee
        30 => FieldEncoding::Fixed(9),   // Amount, transaction processing fee
        31 => FieldEncoding::Fixed(9),   // Amount, settlement processing fee
        32 => FieldEncoding::LlVar(11),  // Acquiring institution ID
        33 => FieldEncoding::LlVar(11),  // Forwarding institution ID
        34 => FieldEncoding::LlVar(28),  // PAN extended
        35 => FieldEncoding::LlVar(37),  // Track 2 data
        36 => FieldEncoding::LllVar(104),// Track 3 data
        37 => FieldEncoding::Fixed(12),  // Retrieval reference number
        38 => FieldEncoding::Fixed(6),   // Authorization code
        39 => FieldEncoding::Fixed(2),   // Response code
        40 => FieldEncoding::Fixed(3),   // Service restriction code
        41 => FieldEncoding::Fixed(8),   // Terminal ID
        42 => FieldEncoding::Fixed(15),  // Merchant ID
        43 => FieldEncoding::Fixed(40),  // Merchant name/location
        44 => FieldEncoding::LlVar(25),  // Additional response data
        45 => FieldEncoding::LlVar(76),  // Track 1 data
        46 => FieldEncoding::LllVar(999),// Additional data (ISO)
        47 => FieldEncoding::LllVar(999),// Additional data (national)
        48 => FieldEncoding::LllVar(999),// Additional data (private)
        49 => FieldEncoding::Fixed(3),   // Currency code (transaction)
        50 => FieldEncoding::Fixed(3),   // Currency code (settlement)
        51 => FieldEncoding::Fixed(3),   // Currency code (cardholder billing)
        52 => FieldEncoding::Fixed(8),   // PIN data (binary)
        53 => FieldEncoding::Fixed(16),  // Security related control info
        54 => FieldEncoding::LllVar(120),// Additional amounts
        55 => FieldEncoding::LllVar(999),// ICC/EMV data
        56 => FieldEncoding::LllVar(999),// Reserved (ISO)
        57..=63 => FieldEncoding::LllVar(999), // Reserved (national/private)
        64 => FieldEncoding::Fixed(8),   // MAC (primary)
        65 => FieldEncoding::Fixed(8),   // Bitmap (extended)
        66 => FieldEncoding::Fixed(1),   // Settlement code
        67 => FieldEncoding::Fixed(2),   // Extended payment code
        68..=75 => FieldEncoding::Fixed(10), // Misc amounts/counts
        76..=84 => FieldEncoding::Fixed(10), // Misc credits/debits
        85..=90 => FieldEncoding::Fixed(10), // Authorizations
        91 => FieldEncoding::Fixed(1),   // File update code
        92..=95 => FieldEncoding::Fixed(2), // Misc indicators
        96 => FieldEncoding::Fixed(8),   // Message security code (binary)
        97 => FieldEncoding::Fixed(17),  // Net settlement amount
        98 => FieldEncoding::Fixed(25),  // Payee name
        99 => FieldEncoding::LlVar(11),  // Settlement institution ID
        100 => FieldEncoding::LlVar(11), // Receiving institution ID
        101 => FieldEncoding::LlVar(17), // File name
        102 => FieldEncoding::LlVar(28), // Account ID 1
        103 => FieldEncoding::LlVar(28), // Account ID 2
        104 => FieldEncoding::LllVar(100),// Transaction description
        105..=107 => FieldEncoding::LllVar(999), // Reserved
        108..=110 => FieldEncoding::LllVar(999), // Reserved
        111..=119 => FieldEncoding::LllVar(999), // Reserved
        120..=122 => FieldEncoding::LllVar(999), // Reserved
        123..=125 => FieldEncoding::LllVar(999), // Reserved
        126 => FieldEncoding::LllVar(999), // Private use
        127 => FieldEncoding::LllVar(999), // Private use
        128 => FieldEncoding::Fixed(8),  // MAC (secondary)
        _ => return None,
    })
}

/// Encode a single field value into the wire format.
fn encode_field(id: u8, value: &[u8]) -> IsoResult<Vec<u8>> {
    match field_encoding(id) {
        Some(FieldEncoding::Fixed(len)) => {
            let mut buf = vec![b' '; len];
            let copy_len = value.len().min(len);
            buf[..copy_len].copy_from_slice(&value[..copy_len]);
            Ok(buf)
        }
        Some(FieldEncoding::LlVar(max)) => {
            if value.len() > max {
                return Err(IsoError::InvalidFormat(
                    format!("Field {id}: value length {} exceeds max {max}", value.len())
                ));
            }
            let mut buf = format!("{:02}", value.len()).into_bytes();
            buf.extend_from_slice(value);
            Ok(buf)
        }
        Some(FieldEncoding::LllVar(max)) => {
            if value.len() > max {
                return Err(IsoError::InvalidFormat(
                    format!("Field {id}: value length {} exceeds max {max}", value.len())
                ));
            }
            let mut buf = format!("{:03}", value.len()).into_bytes();
            buf.extend_from_slice(value);
            Ok(buf)
        }
        None => Err(IsoError::InvalidFormat(format!("Unknown field id: {id}"))),
    }
}

/// Decode a single field from a byte slice, advancing `pos`.
fn decode_field(id: u8, data: &[u8], pos: &mut usize) -> IsoResult<Vec<u8>> {
    match field_encoding(id) {
        Some(FieldEncoding::Fixed(len)) => {
            if *pos + len > data.len() {
                return Err(IsoError::InvalidFormat(
                    format!("Field {id}: unexpected end of data")
                ));
            }
            let value = data[*pos..*pos + len].to_vec();
            *pos += len;
            Ok(value)
        }
        Some(FieldEncoding::LlVar(_)) => {
            if *pos + 2 > data.len() {
                return Err(IsoError::InvalidFormat(format!("Field {id}: missing LL prefix")));
            }
            let len_str = std::str::from_utf8(&data[*pos..*pos + 2])
                .map_err(|_| IsoError::InvalidFormat(format!("Field {id}: invalid LL prefix")))?;
            let len: usize = len_str.parse()
                .map_err(|_| IsoError::InvalidFormat(format!("Field {id}: non-numeric LL prefix")))?;
            *pos += 2;
            if *pos + len > data.len() {
                return Err(IsoError::InvalidFormat(format!("Field {id}: data shorter than declared length")));
            }
            let value = data[*pos..*pos + len].to_vec();
            *pos += len;
            Ok(value)
        }
        Some(FieldEncoding::LllVar(_)) => {
            if *pos + 3 > data.len() {
                return Err(IsoError::InvalidFormat(format!("Field {id}: missing LLL prefix")));
            }
            let len_str = std::str::from_utf8(&data[*pos..*pos + 3])
                .map_err(|_| IsoError::InvalidFormat(format!("Field {id}: invalid LLL prefix")))?;
            let len: usize = len_str.parse()
                .map_err(|_| IsoError::InvalidFormat(format!("Field {id}: non-numeric LLL prefix")))?;
            *pos += 3;
            if *pos + len > data.len() {
                return Err(IsoError::InvalidFormat(format!("Field {id}: data shorter than declared length")));
            }
            let value = data[*pos..*pos + len].to_vec();
            *pos += len;
            Ok(value)
        }
        None => Err(IsoError::InvalidFormat(format!("Unknown field id: {id}"))),
    }
}

/// Parse ISO 8583 binary message
pub fn parse_iso8583(data: &[u8]) -> IsoResult<Iso8583Message> {
    if data.len() < 10 {
        return Err(IsoError::InvalidFormat("Message too short".into()));
    }

    // MTI: 4 bytes ASCII (e.g. "0100") or 2 bytes binary.
    let (mti_code, mut pos) = if data[0].is_ascii_digit() {
        let mti_str = std::str::from_utf8(&data[0..4])
            .map_err(|_| IsoError::InvalidFormat("Invalid MTI".into()))?;
        let code = u16::from_str_radix(mti_str, 16)
            .map_err(|_| IsoError::InvalidFormat("Invalid MTI value".into()))?;
        (code, 4)
    } else {
        (u16::from_be_bytes([data[0], data[1]]), 2)
    };

    let mti = MessageType::from_code(mti_code)
        .ok_or_else(|| IsoError::UnknownMessageType(format!("{:04X}", mti_code)))?;

    let mut msg = Iso8583Message::new(mti);

    // Primary bitmap: 8 bytes.
    if pos + 8 > data.len() {
        return Err(IsoError::InvalidFormat("Missing primary bitmap".into()));
    }
    msg.primary_bitmap = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
    pos += 8;

    // Secondary bitmap: present when bit 1 of primary bitmap is set.
    let has_secondary = (msg.primary_bitmap >> 63) & 1 == 1;
    if has_secondary {
        if pos + 8 > data.len() {
            return Err(IsoError::InvalidFormat("Missing secondary bitmap".into()));
        }
        msg.secondary_bitmap = Some(u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap()));
        pos += 8;
    }

    // Decode each set bit as a data element.
    for bit in 1u8..=64u8 {
        if bit == 1 { continue; } // Bit 1 = secondary bitmap indicator, not a data element.
        if (msg.primary_bitmap >> (64 - bit)) & 1 == 1 {
            let value = decode_field(bit, data, &mut pos)?;
            msg.elements.insert(bit, value);
        }
    }
    if let Some(secondary) = msg.secondary_bitmap {
        for bit in 65u8..=128u8 {
            if bit == 65 { continue; } // Bit 65 = tertiary bitmap indicator.
            if (secondary >> (128 - bit)) & 1 == 1 {
                let value = decode_field(bit, data, &mut pos)?;
                msg.elements.insert(bit, value);
            }
        }
    }

    Ok(msg)
}

/// Serialize ISO 8583 message to binary
pub fn serialize_iso8583(msg: &Iso8583Message) -> IsoResult<Vec<u8>> {
    let mut data = Vec::new();

    // MTI (4 bytes ASCII)
    data.extend_from_slice(format!("{:04X}", msg.mti.to_code()).as_bytes());

    // Primary bitmap (8 bytes big-endian)
    data.extend_from_slice(&msg.primary_bitmap.to_be_bytes());

    // Secondary bitmap if present
    if let Some(secondary) = msg.secondary_bitmap {
        data.extend_from_slice(&secondary.to_be_bytes());
    }

    // Data elements in ascending field-number order.
    let mut field_ids: Vec<u8> = msg.elements.keys().cloned().collect();
    field_ids.sort_unstable();

    for id in field_ids {
        if let Some(value) = msg.elements.get(&id) {
            let encoded = encode_field(id, value)?;
            data.extend_from_slice(&encoded);
        }
    }

    Ok(data)
}

/// Create an authorization request
pub fn create_auth_request(
    pan: &str,
    amount: u64,
    currency: &str,
    terminal_id: &str,
    merchant_id: &str,
) -> Iso8583Message {
    let mut msg = Iso8583Message::new(MessageType::AuthorizationRequest);
    
    // Field 2: PAN
    msg.set_element(2, pan.as_bytes().to_vec());
    
    // Field 3: Processing Code (purchase)
    msg.set_element(3, b"000000".to_vec());
    
    // Field 4: Amount
    msg.set_element(4, format!("{:012}", amount).as_bytes().to_vec());
    
    // Field 11: STAN
    msg.set_element(11, b"000001".to_vec());
    
    // Field 22: POS Entry Mode (manual key entry)
    msg.set_element(22, b"012".to_vec());
    
    // Field 41: Terminal ID
    msg.set_element(41, terminal_id.as_bytes().to_vec());
    
    // Field 42: Merchant ID
    msg.set_element(42, merchant_id.as_bytes().to_vec());
    
    // Field 49: Currency Code
    msg.set_element(49, currency.as_bytes().to_vec());
    
    msg
}

/// Create an authorization response
pub fn create_auth_response(
    request: &Iso8583Message,
    response_code: &str,
    auth_code: &str,
) -> Iso8583Message {
    let mut msg = Iso8583Message::new(MessageType::AuthorizationResponse);
    
    // Copy relevant fields from request
    if let Some(pan) = request.get_element(2) {
        msg.set_element(2, pan.clone());
    }
    if let Some(amount) = request.get_element(4) {
        msg.set_element(4, amount.clone());
    }
    if let Some(stan) = request.get_element(11) {
        msg.set_element(11, stan.clone());
    }
    
    // Field 38: Authorization Code
    msg.set_element(38, auth_code.as_bytes().to_vec());
    
    // Field 39: Response Code
    msg.set_element(39, response_code.as_bytes().to_vec());
    
    msg
}

/// ISO 8583 response codes
pub mod response_codes {
    pub const APPROVED: &str = "00";
    pub const REFER_TO_ISSUER: &str = "01";
    pub const INVALID_MERCHANT: &str = "03";
    pub const DO_NOT_HONOR: &str = "05";
    pub const INVALID_TRANSACTION: &str = "12";
    pub const INVALID_AMOUNT: &str = "13";
    pub const INVALID_CARD_NUMBER: &str = "14";
    pub const NO_SUCH_ISSUER: &str = "15";
    pub const EXPIRED_CARD: &str = "54";
    pub const INSUFFICIENT_FUNDS: &str = "51";
    pub const EXCEEDS_LIMIT: &str = "61";
    pub const RESTRICTED_CARD: &str = "62";
    pub const SECURITY_VIOLATION: &str = "63";
    pub const ACTIVITY_LIMIT_EXCEEDED: &str = "65";
    pub const SYSTEM_MALFUNCTION: &str = "96";
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_auth_request() {
        let msg = create_auth_request(
            "4111111111111111",
            10000, // $100.00
            "840", // USD
            "TERM0001",
            "MERCHANT001",
        );
        
        assert_eq!(msg.mti, MessageType::AuthorizationRequest);
        assert_eq!(msg.pan().unwrap(), "4111111111111111");
        assert_eq!(msg.amount().unwrap(), 10000);
    }
    
    #[test]
    fn test_auth_response() {
        let request = create_auth_request(
            "4111111111111111",
            10000,
            "840",
            "TERM0001",
            "MERCHANT001",
        );
        
        let response = create_auth_response(&request, response_codes::APPROVED, "123456");
        
        assert_eq!(response.mti, MessageType::AuthorizationResponse);
        assert_eq!(response.response_code().unwrap(), "00");
        assert_eq!(response.authorization_code().unwrap(), "123456");
    }
}
