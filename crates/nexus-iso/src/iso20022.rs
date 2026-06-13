//! ISO 20022 message handling
//!
//! ISO 20022 is the XML-based financial messaging standard
//! used by SWIFT, SEPA, and most modern payment systems.

use crate::{IsoError, IsoResult};
use chrono::{DateTime, Utc};
use quick_xml::de::from_str;
use quick_xml::se::to_string;
use serde::{Deserialize, Serialize};

/// ISO 20022 message envelope
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "Document")]
pub struct Iso20022Document {
    /// Business application header
    #[serde(rename = "AppHdr")]
    pub app_header: Option<AppHeader>,
    
    /// The actual message content
    #[serde(flatten)]
    pub content: Iso20022Message,
}

/// Business application header (head.001)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppHeader {
    #[serde(rename = "Fr")]
    pub from: Party,
    
    #[serde(rename = "To")]
    pub to: Party,
    
    #[serde(rename = "BizMsgIdr")]
    pub business_message_id: String,
    
    #[serde(rename = "MsgDefIdr")]
    pub message_definition_id: String,
    
    #[serde(rename = "CreDt")]
    pub creation_date: String,
}

/// Party identification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Party {
    #[serde(rename = "FIId")]
    pub fi_id: Option<FinancialInstitutionId>,
    
    #[serde(rename = "OrgId")]
    pub org_id: Option<OrganizationId>,
}

/// Financial institution identification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinancialInstitutionId {
    #[serde(rename = "BICFI")]
    pub bic: Option<String>,
    
    #[serde(rename = "ClrSysMmbId")]
    pub clearing_system_member_id: Option<ClearingSystemMemberId>,
    
    #[serde(rename = "LEI")]
    pub lei: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClearingSystemMemberId {
    #[serde(rename = "ClrSysId")]
    pub clearing_system_id: Option<ClearingSystemId>,
    
    #[serde(rename = "MmbId")]
    pub member_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClearingSystemId {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizationId {
    #[serde(rename = "Nm")]
    pub name: Option<String>,
    
    #[serde(rename = "LEI")]
    pub lei: Option<String>,
    
    #[serde(rename = "Othr")]
    pub other: Option<Vec<GenericId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenericId {
    #[serde(rename = "Id")]
    pub id: String,
    
    #[serde(rename = "SchmeNm")]
    pub scheme_name: Option<SchemeName>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemeName {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

/// ISO 20022 message types
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Iso20022Message {
    /// pacs.008 - FI to FI Customer Credit Transfer
    CreditTransfer(CreditTransferMessage),
    
    /// pacs.002 - Payment Status Report
    PaymentStatus(PaymentStatusMessage),
    
    /// camt.053 - Bank to Customer Statement
    Statement(StatementMessage),
}

/// pacs.008 - Credit Transfer
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "FIToFICstmrCdtTrf")]
pub struct CreditTransferMessage {
    #[serde(rename = "GrpHdr")]
    pub group_header: GroupHeader,
    
    #[serde(rename = "CdtTrfTxInf")]
    pub credit_transfer_transactions: Vec<CreditTransferTransaction>,
}

/// Group header for payment messages
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupHeader {
    #[serde(rename = "MsgId")]
    pub message_id: String,
    
    #[serde(rename = "CreDtTm")]
    pub creation_date_time: String,
    
    #[serde(rename = "NbOfTxs")]
    pub number_of_transactions: u32,
    
    #[serde(rename = "TtlIntrBkSttlmAmt")]
    pub total_amount: Option<Amount>,
    
    #[serde(rename = "IntrBkSttlmDt")]
    pub settlement_date: Option<String>,
    
    #[serde(rename = "SttlmInf")]
    pub settlement_info: Option<SettlementInfo>,
    
    #[serde(rename = "InstgAgt")]
    pub instructing_agent: Option<FinancialInstitutionId>,
    
    #[serde(rename = "InstdAgt")]
    pub instructed_agent: Option<FinancialInstitutionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Amount {
    #[serde(rename = "@Ccy")]
    pub currency: String,
    
    #[serde(rename = "$value")]
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementInfo {
    #[serde(rename = "SttlmMtd")]
    pub settlement_method: String,
    
    #[serde(rename = "SttlmAcct")]
    pub settlement_account: Option<AccountId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountId {
    #[serde(rename = "IBAN")]
    pub iban: Option<String>,
    
    #[serde(rename = "Othr")]
    pub other: Option<GenericId>,
}

/// Credit transfer transaction information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreditTransferTransaction {
    #[serde(rename = "PmtId")]
    pub payment_id: PaymentId,
    
    #[serde(rename = "PmtTpInf")]
    pub payment_type_info: Option<PaymentTypeInfo>,
    
    #[serde(rename = "IntrBkSttlmAmt")]
    pub interbank_settlement_amount: Amount,
    
    #[serde(rename = "InstdAmt")]
    pub instructed_amount: Option<Amount>,
    
    #[serde(rename = "ChrgBr")]
    pub charge_bearer: Option<String>,
    
    #[serde(rename = "Dbtr")]
    pub debtor: PartyInfo,
    
    #[serde(rename = "DbtrAcct")]
    pub debtor_account: AccountId,
    
    #[serde(rename = "DbtrAgt")]
    pub debtor_agent: FinancialInstitutionId,
    
    #[serde(rename = "CdtrAgt")]
    pub creditor_agent: FinancialInstitutionId,
    
    #[serde(rename = "Cdtr")]
    pub creditor: PartyInfo,
    
    #[serde(rename = "CdtrAcct")]
    pub creditor_account: AccountId,
    
    #[serde(rename = "RmtInf")]
    pub remittance_info: Option<RemittanceInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentId {
    #[serde(rename = "InstrId")]
    pub instruction_id: Option<String>,
    
    #[serde(rename = "EndToEndId")]
    pub end_to_end_id: String,
    
    #[serde(rename = "TxId")]
    pub transaction_id: Option<String>,
    
    #[serde(rename = "UETR")]
    pub uetr: Option<String>, // Unique End-to-end Transaction Reference
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentTypeInfo {
    #[serde(rename = "InstrPrty")]
    pub instruction_priority: Option<String>,
    
    #[serde(rename = "ClrChanl")]
    pub clearing_channel: Option<String>,
    
    #[serde(rename = "SvcLvl")]
    pub service_level: Option<ServiceLevel>,
    
    #[serde(rename = "LclInstrm")]
    pub local_instrument: Option<LocalInstrument>,
    
    #[serde(rename = "CtgyPurp")]
    pub category_purpose: Option<CategoryPurpose>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceLevel {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalInstrument {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoryPurpose {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyInfo {
    #[serde(rename = "Nm")]
    pub name: Option<String>,
    
    #[serde(rename = "PstlAdr")]
    pub postal_address: Option<PostalAddress>,
    
    #[serde(rename = "Id")]
    pub id: Option<PartyIdentification>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostalAddress {
    #[serde(rename = "StrtNm")]
    pub street_name: Option<String>,
    
    #[serde(rename = "BldgNb")]
    pub building_number: Option<String>,
    
    #[serde(rename = "PstCd")]
    pub postal_code: Option<String>,
    
    #[serde(rename = "TwnNm")]
    pub town_name: Option<String>,
    
    #[serde(rename = "CtrySubDvsn")]
    pub country_subdivision: Option<String>,
    
    #[serde(rename = "Ctry")]
    pub country: Option<String>,
    
    #[serde(rename = "AdrLine")]
    pub address_lines: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyIdentification {
    #[serde(rename = "OrgId")]
    pub org_id: Option<OrganizationId>,
    
    #[serde(rename = "PrvtId")]
    pub private_id: Option<PrivateId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateId {
    #[serde(rename = "DtAndPlcOfBirth")]
    pub date_and_place_of_birth: Option<DateAndPlaceOfBirth>,
    
    #[serde(rename = "Othr")]
    pub other: Option<Vec<GenericId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DateAndPlaceOfBirth {
    #[serde(rename = "BirthDt")]
    pub birth_date: String,
    
    #[serde(rename = "CityOfBirth")]
    pub city_of_birth: Option<String>,
    
    #[serde(rename = "CtryOfBirth")]
    pub country_of_birth: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemittanceInfo {
    #[serde(rename = "Ustrd")]
    pub unstructured: Option<Vec<String>>,
    
    #[serde(rename = "Strd")]
    pub structured: Option<Vec<StructuredRemittanceInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuredRemittanceInfo {
    #[serde(rename = "RfrdDocInf")]
    pub referred_document_info: Option<ReferredDocumentInfo>,
    
    #[serde(rename = "RfrdDocAmt")]
    pub referred_document_amount: Option<ReferredDocumentAmount>,
    
    #[serde(rename = "CdtrRefInf")]
    pub creditor_reference_info: Option<CreditorReferenceInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferredDocumentInfo {
    #[serde(rename = "Tp")]
    pub doc_type: Option<ReferredDocumentType>,
    
    #[serde(rename = "Nb")]
    pub number: Option<String>,
    
    #[serde(rename = "RltdDt")]
    pub related_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferredDocumentType {
    #[serde(rename = "CdOrPrtry")]
    pub code_or_proprietary: CodeOrProprietary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeOrProprietary {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferredDocumentAmount {
    #[serde(rename = "DuePyblAmt")]
    pub due_payable_amount: Option<Amount>,
    
    #[serde(rename = "RmtdAmt")]
    pub remitted_amount: Option<Amount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreditorReferenceInfo {
    #[serde(rename = "Tp")]
    pub ref_type: Option<CreditorReferenceType>,
    
    #[serde(rename = "Ref")]
    pub reference: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreditorReferenceType {
    #[serde(rename = "CdOrPrtry")]
    pub code_or_proprietary: CodeOrProprietary,
}

/// pacs.002 - Payment Status Report
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "FIToFIPmtStsRpt")]
pub struct PaymentStatusMessage {
    #[serde(rename = "GrpHdr")]
    pub group_header: StatusGroupHeader,
    
    #[serde(rename = "OrgnlGrpInfAndSts")]
    pub original_group_info: Option<OriginalGroupInfo>,
    
    #[serde(rename = "TxInfAndSts")]
    pub transaction_info_and_status: Option<Vec<TransactionStatus>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusGroupHeader {
    #[serde(rename = "MsgId")]
    pub message_id: String,
    
    #[serde(rename = "CreDtTm")]
    pub creation_date_time: String,
    
    #[serde(rename = "InstgAgt")]
    pub instructing_agent: Option<FinancialInstitutionId>,
    
    #[serde(rename = "InstdAgt")]
    pub instructed_agent: Option<FinancialInstitutionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OriginalGroupInfo {
    #[serde(rename = "OrgnlMsgId")]
    pub original_message_id: String,
    
    #[serde(rename = "OrgnlMsgNmId")]
    pub original_message_name_id: String,
    
    #[serde(rename = "OrgnlCreDtTm")]
    pub original_creation_date_time: Option<String>,
    
    #[serde(rename = "GrpSts")]
    pub group_status: Option<String>,
    
    #[serde(rename = "StsRsnInf")]
    pub status_reason_info: Option<Vec<StatusReasonInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionStatus {
    #[serde(rename = "OrgnlInstrId")]
    pub original_instruction_id: Option<String>,
    
    #[serde(rename = "OrgnlEndToEndId")]
    pub original_end_to_end_id: Option<String>,
    
    #[serde(rename = "OrgnlTxId")]
    pub original_transaction_id: Option<String>,
    
    #[serde(rename = "TxSts")]
    pub transaction_status: Option<String>,
    
    #[serde(rename = "StsRsnInf")]
    pub status_reason_info: Option<Vec<StatusReasonInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusReasonInfo {
    #[serde(rename = "Rsn")]
    pub reason: Option<StatusReason>,
    
    #[serde(rename = "AddtlInf")]
    pub additional_info: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusReason {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

/// camt.053 - Bank to Customer Statement
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "BkToCstmrStmt")]
pub struct StatementMessage {
    #[serde(rename = "GrpHdr")]
    pub group_header: StatementGroupHeader,
    
    #[serde(rename = "Stmt")]
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatementGroupHeader {
    #[serde(rename = "MsgId")]
    pub message_id: String,
    
    #[serde(rename = "CreDtTm")]
    pub creation_date_time: String,
    
    #[serde(rename = "MsgRcpt")]
    pub message_recipient: Option<PartyInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Statement {
    #[serde(rename = "Id")]
    pub id: String,
    
    #[serde(rename = "StmtPgntn")]
    pub pagination: Option<Pagination>,
    
    #[serde(rename = "ElctrncSeqNb")]
    pub electronic_sequence_number: Option<u64>,
    
    #[serde(rename = "CreDtTm")]
    pub creation_date_time: String,
    
    #[serde(rename = "FrToDt")]
    pub from_to_date: Option<FromToDate>,
    
    #[serde(rename = "Acct")]
    pub account: CashAccount,
    
    #[serde(rename = "Bal")]
    pub balances: Vec<Balance>,
    
    #[serde(rename = "TxsSummry")]
    pub transactions_summary: Option<TransactionsSummary>,
    
    #[serde(rename = "Ntry")]
    pub entries: Option<Vec<Entry>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pagination {
    #[serde(rename = "PgNb")]
    pub page_number: String,
    
    #[serde(rename = "LastPgInd")]
    pub last_page_indicator: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FromToDate {
    #[serde(rename = "FrDtTm")]
    pub from_date_time: String,
    
    #[serde(rename = "ToDtTm")]
    pub to_date_time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CashAccount {
    #[serde(rename = "Id")]
    pub id: AccountId,
    
    #[serde(rename = "Tp")]
    pub account_type: Option<AccountType>,
    
    #[serde(rename = "Ccy")]
    pub currency: Option<String>,
    
    #[serde(rename = "Nm")]
    pub name: Option<String>,
    
    #[serde(rename = "Ownr")]
    pub owner: Option<PartyInfo>,
    
    #[serde(rename = "Svcr")]
    pub servicer: Option<FinancialInstitutionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountType {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Balance {
    #[serde(rename = "Tp")]
    pub balance_type: BalanceType,
    
    #[serde(rename = "Amt")]
    pub amount: Amount,
    
    #[serde(rename = "CdtDbtInd")]
    pub credit_debit_indicator: String,
    
    #[serde(rename = "Dt")]
    pub date: BalanceDate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceType {
    #[serde(rename = "CdOrPrtry")]
    pub code_or_proprietary: CodeOrProprietary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceDate {
    #[serde(rename = "Dt")]
    pub date: Option<String>,
    
    #[serde(rename = "DtTm")]
    pub date_time: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionsSummary {
    #[serde(rename = "TtlNtries")]
    pub total_entries: Option<TotalEntries>,
    
    #[serde(rename = "TtlCdtNtries")]
    pub total_credit_entries: Option<TotalEntries>,
    
    #[serde(rename = "TtlDbtNtries")]
    pub total_debit_entries: Option<TotalEntries>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotalEntries {
    #[serde(rename = "NbOfNtries")]
    pub number_of_entries: Option<u64>,
    
    #[serde(rename = "Sum")]
    pub sum: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "Amt")]
    pub amount: Amount,
    
    #[serde(rename = "CdtDbtInd")]
    pub credit_debit_indicator: String,
    
    #[serde(rename = "Sts")]
    pub status: Option<EntryStatus>,
    
    #[serde(rename = "BookgDt")]
    pub booking_date: Option<BalanceDate>,
    
    #[serde(rename = "ValDt")]
    pub value_date: Option<BalanceDate>,
    
    #[serde(rename = "BkTxCd")]
    pub bank_transaction_code: Option<BankTransactionCode>,
    
    #[serde(rename = "NtryDtls")]
    pub entry_details: Option<Vec<EntryDetails>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryStatus {
    #[serde(rename = "Cd")]
    pub code: Option<String>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BankTransactionCode {
    #[serde(rename = "Domn")]
    pub domain: Option<Domain>,
    
    #[serde(rename = "Prtry")]
    pub proprietary: Option<ProprietaryCode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Domain {
    #[serde(rename = "Cd")]
    pub code: String,
    
    #[serde(rename = "Fmly")]
    pub family: DomainFamily,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainFamily {
    #[serde(rename = "Cd")]
    pub code: String,
    
    #[serde(rename = "SubFmlyCd")]
    pub sub_family_code: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProprietaryCode {
    #[serde(rename = "Cd")]
    pub code: String,
    
    #[serde(rename = "Issr")]
    pub issuer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryDetails {
    #[serde(rename = "TxDtls")]
    pub transaction_details: Option<Vec<TransactionDetails>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionDetails {
    #[serde(rename = "Refs")]
    pub references: Option<TransactionReferences>,
    
    #[serde(rename = "Amt")]
    pub amount: Option<Amount>,
    
    #[serde(rename = "RltdPties")]
    pub related_parties: Option<RelatedParties>,
    
    #[serde(rename = "RmtInf")]
    pub remittance_info: Option<RemittanceInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionReferences {
    #[serde(rename = "MsgId")]
    pub message_id: Option<String>,
    
    #[serde(rename = "EndToEndId")]
    pub end_to_end_id: Option<String>,
    
    #[serde(rename = "TxId")]
    pub transaction_id: Option<String>,
    
    #[serde(rename = "UETR")]
    pub uetr: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelatedParties {
    #[serde(rename = "Dbtr")]
    pub debtor: Option<PartyInfo>,
    
    #[serde(rename = "DbtrAcct")]
    pub debtor_account: Option<AccountId>,
    
    #[serde(rename = "Cdtr")]
    pub creditor: Option<PartyInfo>,
    
    #[serde(rename = "CdtrAcct")]
    pub creditor_account: Option<AccountId>,
}

/// Parse ISO 20022 XML message
pub fn parse_iso20022(xml: &str) -> IsoResult<Iso20022Document> {
    from_str(xml).map_err(|e| IsoError::XmlError(e.to_string()))
}

/// Serialize ISO 20022 message to XML
pub fn serialize_iso20022(doc: &Iso20022Document) -> IsoResult<String> {
    to_string(doc).map_err(|e| IsoError::XmlError(e.to_string()))
}

/// Create a credit transfer message (pacs.008)
pub fn create_credit_transfer(
    message_id: &str,
    debtor_name: &str,
    debtor_iban: &str,
    debtor_bic: &str,
    creditor_name: &str,
    creditor_iban: &str,
    creditor_bic: &str,
    amount: &str,
    currency: &str,
    reference: &str,
) -> CreditTransferMessage {
    let now = chrono::Utc::now();
    
    CreditTransferMessage {
        group_header: GroupHeader {
            message_id: message_id.to_string(),
            creation_date_time: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
            number_of_transactions: 1,
            total_amount: Some(Amount {
                currency: currency.to_string(),
                value: amount.to_string(),
            }),
            settlement_date: Some(now.format("%Y-%m-%d").to_string()),
            settlement_info: Some(SettlementInfo {
                settlement_method: "INDA".to_string(),
                settlement_account: None,
            }),
            instructing_agent: Some(FinancialInstitutionId {
                bic: Some(debtor_bic.to_string()),
                clearing_system_member_id: None,
                lei: None,
            }),
            instructed_agent: Some(FinancialInstitutionId {
                bic: Some(creditor_bic.to_string()),
                clearing_system_member_id: None,
                lei: None,
            }),
        },
        credit_transfer_transactions: vec![CreditTransferTransaction {
            payment_id: PaymentId {
                instruction_id: Some(format!("{}-001", message_id)),
                end_to_end_id: reference.to_string(),
                transaction_id: Some(format!("{}-TX001", message_id)),
                uetr: Some(uuid_v4()),
            },
            payment_type_info: None,
            interbank_settlement_amount: Amount {
                currency: currency.to_string(),
                value: amount.to_string(),
            },
            instructed_amount: None,
            charge_bearer: Some("SHAR".to_string()),
            debtor: PartyInfo {
                name: Some(debtor_name.to_string()),
                postal_address: None,
                id: None,
            },
            debtor_account: AccountId {
                iban: Some(debtor_iban.to_string()),
                other: None,
            },
            debtor_agent: FinancialInstitutionId {
                bic: Some(debtor_bic.to_string()),
                clearing_system_member_id: None,
                lei: None,
            },
            creditor_agent: FinancialInstitutionId {
                bic: Some(creditor_bic.to_string()),
                clearing_system_member_id: None,
                lei: None,
            },
            creditor: PartyInfo {
                name: Some(creditor_name.to_string()),
                postal_address: None,
                id: None,
            },
            creditor_account: AccountId {
                iban: Some(creditor_iban.to_string()),
                other: None,
            },
            remittance_info: Some(RemittanceInfo {
                unstructured: Some(vec![reference.to_string()]),
                structured: None,
            }),
        }],
    }
}

/// Generate UUID v4 (simplified)
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (now >> 96) as u32,
        ((now >> 80) & 0xFFFF) as u16,
        ((now >> 68) & 0x0FFF) as u16,
        (((now >> 52) & 0x3FFF) | 0x8000) as u16,
        (now & 0xFFFFFFFFFFFF) as u64
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_credit_transfer() {
        let msg = create_credit_transfer(
            "MSG001",
            "John Doe",
            "DE89370400440532013000",
            "COBADEFFXXX",
            "Jane Smith",
            "GB33BUKB20201555555555",
            "BUKBGB22XXX",
            "1000.00",
            "EUR",
            "Invoice 12345",
        );
        
        assert_eq!(msg.group_header.number_of_transactions, 1);
        assert_eq!(msg.credit_transfer_transactions.len(), 1);
        
        let tx = &msg.credit_transfer_transactions[0];
        assert_eq!(tx.debtor.name.as_ref().unwrap(), "John Doe");
        assert_eq!(tx.interbank_settlement_amount.value, "1000.00");
    }
}
