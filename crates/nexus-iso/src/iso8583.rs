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

/// Parse ISO 8583 binary message
pub fn parse_iso8583(data: &[u8]) -> IsoResult<Iso8583Message> {
    if data.len() < 10 {
        return Err(IsoError::InvalidFormat("Message too short".into()));
    }
    
    // Parse MTI (4 bytes ASCII or 2 bytes binary)
    let mti_code = if data[0].is_ascii_digit() {
        // ASCII format
        let mti_str = std::str::from_utf8(&data[0..4])
            .map_err(|_| IsoError::InvalidFormat("Invalid MTI".into()))?;
        u16::from_str_radix(mti_str, 16)
            .map_err(|_| IsoError::InvalidFormat("Invalid MTI value".into()))?
    } else {
        // Binary format
        u16::from_be_bytes([data[0], data[1]])
    };
    
    let mti = MessageType::from_code(mti_code)
        .ok_or_else(|| IsoError::UnknownMessageType(format!("{:04X}", mti_code)))?;
    
    let mut msg = Iso8583Message::new(mti);
    
    // Parse bitmaps and fields based on message specification
    // This is simplified - full implementation would handle all field types
    
    Ok(msg)
}

/// Serialize ISO 8583 message to binary
pub fn serialize_iso8583(msg: &Iso8583Message) -> IsoResult<Vec<u8>> {
    let mut data = Vec::new();
    
    // MTI (4 bytes ASCII)
    data.extend_from_slice(format!("{:04X}", msg.mti.to_code()).as_bytes());
    
    // Primary bitmap (8 bytes)
    data.extend_from_slice(&msg.primary_bitmap.to_be_bytes());
    
    // Secondary bitmap if present
    if let Some(secondary) = msg.secondary_bitmap {
        data.extend_from_slice(&secondary.to_be_bytes());
    }
    
    // Data elements (sorted by field number)
    let mut field_ids: Vec<u8> = msg.elements.keys().cloned().collect();
    field_ids.sort();
    
    for id in field_ids {
        if let Some(value) = msg.elements.get(&id) {
            // Field format depends on field type (this is simplified)
            data.extend_from_slice(value);
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
