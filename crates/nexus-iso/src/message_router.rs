//! Message routing for ISO compliance

use crate::{IsoError, IsoResult, iso20022::*, iso8583::*};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Message type enumeration
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    /// ISO 20022 Credit Transfer (pacs.008)
    CreditTransfer,
    /// ISO 20022 Payment Status (pacs.002)
    PaymentStatus,
    /// ISO 20022 Direct Debit (pacs.003)
    DirectDebit,
    /// ISO 20022 Return (pacs.004)
    PaymentReturn,
    /// ISO 8583 Authorization
    CardAuthorization,
    /// ISO 8583 Financial
    CardFinancial,
    /// ISO 8583 Reversal
    CardReversal,
    /// Internal blockchain transaction
    BlockchainTx,
}

/// Routing destination
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteDestination {
    /// Destination identifier (BIC, endpoint URL, etc.)
    pub destination_id: String,
    /// Destination type
    pub destination_type: DestinationType,
    /// Priority (lower = higher priority)
    pub priority: u8,
    /// Active status
    pub active: bool,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DestinationType {
    Swift,
    Sepa,
    Fedwire,
    Blockchain,
    Internal,
    Api,
}

/// Routing rule
#[derive(Clone, Debug)]
pub struct RoutingRule {
    /// Rule identifier
    pub id: String,
    /// Rule name
    pub name: String,
    /// Message type this rule applies to
    pub message_type: MessageType,
    /// Condition to match
    pub condition: RoutingCondition,
    /// Destinations when rule matches
    pub destinations: Vec<RouteDestination>,
    /// Rule priority
    pub priority: u8,
}

/// Routing condition
#[derive(Clone, Debug)]
pub enum RoutingCondition {
    /// Always match
    Always,
    /// Match by currency
    Currency(String),
    /// Match by amount threshold
    AmountGreaterThan(u64),
    /// Match by country
    CreditorCountry(String),
    DebtorCountry(String),
    /// Match by BIC prefix
    BicPrefix(String),
    /// Compound AND condition
    And(Box<RoutingCondition>, Box<RoutingCondition>),
    /// Compound OR condition
    Or(Box<RoutingCondition>, Box<RoutingCondition>),
    /// Negate condition
    Not(Box<RoutingCondition>),
}

impl RoutingCondition {
    /// Evaluate condition against message context
    pub fn matches(&self, ctx: &MessageContext) -> bool {
        match self {
            RoutingCondition::Always => true,
            RoutingCondition::Currency(c) => ctx.currency.as_deref() == Some(c.as_str()),
            RoutingCondition::AmountGreaterThan(amt) => ctx.amount.map_or(false, |a| a > *amt),
            RoutingCondition::CreditorCountry(c) => ctx.creditor_country.as_deref() == Some(c.as_str()),
            RoutingCondition::DebtorCountry(c) => ctx.debtor_country.as_deref() == Some(c.as_str()),
            RoutingCondition::BicPrefix(prefix) => ctx.creditor_bic
                .as_ref()
                .map_or(false, |bic| bic.starts_with(prefix)),
            RoutingCondition::And(a, b) => a.matches(ctx) && b.matches(ctx),
            RoutingCondition::Or(a, b) => a.matches(ctx) || b.matches(ctx),
            RoutingCondition::Not(c) => !c.matches(ctx),
        }
    }
}

/// Context extracted from message for routing decisions
#[derive(Clone, Debug, Default)]
pub struct MessageContext {
    pub message_type: Option<MessageType>,
    pub currency: Option<String>,
    pub amount: Option<u64>,
    pub creditor_country: Option<String>,
    pub debtor_country: Option<String>,
    pub creditor_bic: Option<String>,
    pub debtor_bic: Option<String>,
    pub creditor_iban: Option<String>,
    pub debtor_iban: Option<String>,
}

impl From<&CreditTransferMessage> for MessageContext {
    fn from(msg: &CreditTransferMessage) -> Self {
        MessageContext {
            message_type: Some(MessageType::CreditTransfer),
            currency: Some(msg.currency.clone()),
            amount: Some(msg.amount),
            creditor_country: msg.creditor_account.as_ref()
                .and_then(|a| a.get(0..2).map(|s| s.to_string())),
            debtor_country: msg.debtor_account.as_ref()
                .and_then(|a| a.get(0..2).map(|s| s.to_string())),
            creditor_bic: msg.creditor_agent_bic.clone(),
            debtor_bic: msg.debtor_agent_bic.clone(),
            creditor_iban: msg.creditor_account.clone(),
            debtor_iban: msg.debtor_account.clone(),
        }
    }
}

impl From<&Iso8583Message> for MessageContext {
    fn from(msg: &Iso8583Message) -> Self {
        MessageContext {
            message_type: Some(match &msg.mti {
                mti if mti.starts_with("01") => MessageType::CardAuthorization,
                mti if mti.starts_with("02") => MessageType::CardFinancial,
                mti if mti.starts_with("04") => MessageType::CardReversal,
                _ => MessageType::CardAuthorization,
            }),
            currency: msg.fields.get(&49).cloned(),
            amount: msg.fields.get(&4).and_then(|a| a.parse().ok()),
            ..Default::default()
        }
    }
}

/// Message router
pub struct MessageRouter {
    rules: Arc<RwLock<Vec<RoutingRule>>>,
    default_routes: Arc<RwLock<HashMap<MessageType, Vec<RouteDestination>>>>,
}

impl MessageRouter {
    /// Create new router
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            default_routes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add routing rule
    pub async fn add_rule(&self, rule: RoutingRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
        rules.sort_by_key(|r| r.priority);
    }

    /// Set default route for message type
    pub async fn set_default_route(&self, msg_type: MessageType, destinations: Vec<RouteDestination>) {
        let mut defaults = self.default_routes.write().await;
        defaults.insert(msg_type, destinations);
    }

    /// Route a message
    pub async fn route(&self, ctx: &MessageContext) -> IsoResult<Vec<RouteDestination>> {
        let rules = self.rules.read().await;
        
        // Find first matching rule
        for rule in rules.iter() {
            if let Some(msg_type) = &ctx.message_type {
                if rule.message_type == *msg_type && rule.condition.matches(ctx) {
                    let active: Vec<_> = rule.destinations.iter()
                        .filter(|d| d.active)
                        .cloned()
                        .collect();
                    
                    if !active.is_empty() {
                        return Ok(active);
                    }
                }
            }
        }
        
        // Fall back to default route
        let defaults = self.default_routes.read().await;
        if let Some(msg_type) = &ctx.message_type {
            if let Some(dests) = defaults.get(msg_type) {
                let active: Vec<_> = dests.iter()
                    .filter(|d| d.active)
                    .cloned()
                    .collect();
                
                if !active.is_empty() {
                    return Ok(active);
                }
            }
        }
        
        Err(IsoError::RoutingFailed("No route found".into()))
    }

    /// Route credit transfer message
    pub async fn route_credit_transfer(&self, msg: &CreditTransferMessage) -> IsoResult<Vec<RouteDestination>> {
        let ctx = MessageContext::from(msg);
        self.route(&ctx).await
    }

    /// Route ISO 8583 message
    pub async fn route_iso8583(&self, msg: &Iso8583Message) -> IsoResult<Vec<RouteDestination>> {
        let ctx = MessageContext::from(msg);
        self.route(&ctx).await
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Compliance check result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplianceResult {
    pub passed: bool,
    pub checks: Vec<ComplianceCheck>,
    pub risk_score: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplianceCheck {
    pub name: String,
    pub passed: bool,
    pub message: Option<String>,
}

/// Compliance checker for regulatory requirements
pub struct ComplianceChecker {
    /// Sanctioned countries (OFAC, EU, etc.)
    sanctioned_countries: Vec<String>,
    /// High-risk countries
    high_risk_countries: Vec<String>,
    /// Amount threshold requiring enhanced due diligence
    edd_threshold: u64,
}

impl ComplianceChecker {
    pub fn new() -> Self {
        Self {
            sanctioned_countries: vec![
                "KP".into(), "IR".into(), "SY".into(), "CU".into(),
            ],
            high_risk_countries: vec![
                "AF".into(), "BY".into(), "MM".into(), "RU".into(), "VE".into(),
            ],
            edd_threshold: 10_000_00, // $10,000 in cents
        }
    }

    /// Check compliance for a message
    pub fn check(&self, ctx: &MessageContext) -> ComplianceResult {
        let mut checks = Vec::new();
        let mut risk_score = 0u8;

        // Sanctioned country check
        let debtor_sanctioned = ctx.debtor_country.as_ref()
            .map_or(false, |c| self.sanctioned_countries.contains(c));
        let creditor_sanctioned = ctx.creditor_country.as_ref()
            .map_or(false, |c| self.sanctioned_countries.contains(c));

        checks.push(ComplianceCheck {
            name: "Sanctions Screening".into(),
            passed: !debtor_sanctioned && !creditor_sanctioned,
            message: if debtor_sanctioned || creditor_sanctioned {
                Some("Transaction involves sanctioned jurisdiction".into())
            } else {
                None
            },
        });

        if debtor_sanctioned || creditor_sanctioned {
            risk_score = 100; // Automatic fail
        }

        // High-risk country check
        let high_risk = ctx.debtor_country.as_ref()
            .map_or(false, |c| self.high_risk_countries.contains(c))
            || ctx.creditor_country.as_ref()
                .map_or(false, |c| self.high_risk_countries.contains(c));

        checks.push(ComplianceCheck {
            name: "High Risk Jurisdiction".into(),
            passed: true, // Warning only
            message: if high_risk {
                risk_score = risk_score.saturating_add(30);
                Some("Transaction involves high-risk jurisdiction - enhanced due diligence required".into())
            } else {
                None
            },
        });

        // Large transaction check
        let large_transaction = ctx.amount.map_or(false, |a| a >= self.edd_threshold);
        checks.push(ComplianceCheck {
            name: "Large Transaction".into(),
            passed: true, // Warning only
            message: if large_transaction {
                risk_score = risk_score.saturating_add(20);
                Some(format!("Transaction exceeds ${} threshold - reporting required", 
                    self.edd_threshold / 100))
            } else {
                None
            },
        });

        ComplianceResult {
            passed: risk_score < 100,
            checks,
            risk_score,
        }
    }
}

impl Default for ComplianceChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_rules() {
        let router = MessageRouter::new();
        
        // Add rule for EUR SEPA payments
        router.add_rule(RoutingRule {
            id: "sepa-eur".into(),
            name: "SEPA EUR Route".into(),
            message_type: MessageType::CreditTransfer,
            condition: RoutingCondition::Currency("EUR".into()),
            destinations: vec![RouteDestination {
                destination_id: "SEPA_GATEWAY".into(),
                destination_type: DestinationType::Sepa,
                priority: 1,
                active: true,
                metadata: HashMap::new(),
            }],
            priority: 1,
        }).await;

        let ctx = MessageContext {
            message_type: Some(MessageType::CreditTransfer),
            currency: Some("EUR".into()),
            ..Default::default()
        };

        let routes = router.route(&ctx).await.unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].destination_id, "SEPA_GATEWAY");
    }

    #[test]
    fn test_compliance_checker() {
        let checker = ComplianceChecker::new();

        // Normal transaction
        let ctx = MessageContext {
            debtor_country: Some("US".into()),
            creditor_country: Some("GB".into()),
            amount: Some(500_00),
            ..Default::default()
        };
        let result = checker.check(&ctx);
        assert!(result.passed);
        assert!(result.risk_score < 50);

        // Sanctioned country
        let ctx = MessageContext {
            debtor_country: Some("US".into()),
            creditor_country: Some("KP".into()), // North Korea
            amount: Some(500_00),
            ..Default::default()
        };
        let result = checker.check(&ctx);
        assert!(!result.passed);
        assert_eq!(result.risk_score, 100);
    }
}
