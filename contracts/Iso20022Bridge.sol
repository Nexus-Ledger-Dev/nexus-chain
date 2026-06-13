// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title Iso20022Bridge
 * @notice Bridge for ISO 20022 message handling on-chain
 * @dev Enables traditional finance interoperability
 */
contract Iso20022Bridge {
    // ============ Constants ============
    
    // ISO 20022 Parser precompile address (NexusChain extension)
    address constant ISO_PARSER = address(0x0101);
    
    // Message types
    bytes4 constant PACS_008 = bytes4(keccak256("pacs.008")); // Credit Transfer
    bytes4 constant PACS_002 = bytes4(keccak256("pacs.002")); // Status Report
    bytes4 constant CAMT_053 = bytes4(keccak256("camt.053")); // Bank Statement
    bytes4 constant PAIN_001 = bytes4(keccak256("pain.001")); // Payment Initiation
    
    // ============ State Variables ============
    
    struct PaymentInstruction {
        bytes32 messageId;
        bytes4 messageType;
        address debtor;
        address creditor;
        uint256 amount;
        bytes32 currency;
        uint256 timestamp;
        PaymentStatus status;
        bytes rawMessage;
    }
    
    enum PaymentStatus {
        Pending,
        Processing,
        Completed,
        Rejected,
        Returned
    }
    
    mapping(bytes32 => PaymentInstruction) public payments;
    mapping(address => bytes32[]) public userPayments;
    mapping(address => bool) public authorizedParticipants;
    
    address public owner;
    address public complianceOracle;
    
    // ============ Events ============
    
    event PaymentInitiated(
        bytes32 indexed messageId,
        address indexed debtor,
        address indexed creditor,
        uint256 amount,
        bytes32 currency
    );
    
    event PaymentStatusUpdated(
        bytes32 indexed messageId,
        PaymentStatus oldStatus,
        PaymentStatus newStatus
    );
    
    event ParticipantAuthorized(address indexed participant);
    event ParticipantRevoked(address indexed participant);
    
    // ============ Constructor ============
    
    constructor() {
        owner = msg.sender;
    }
    
    // ============ Modifiers ============
    
    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }
    
    modifier onlyAuthorized() {
        require(authorizedParticipants[msg.sender], "Not authorized");
        _;
    }
    
    // ============ External Functions ============
    
    /**
     * @notice Submit a pacs.008 credit transfer message
     * @param xmlMessage The ISO 20022 XML message
     */
    function submitCreditTransfer(bytes calldata xmlMessage) external payable onlyAuthorized {
        // Parse ISO 20022 message using precompile
        (bool success, bytes memory result) = ISO_PARSER.staticcall(
            abi.encode(PACS_008, xmlMessage)
        );
        require(success, "Parse failed");
        
        // Decode parsed fields
        (
            bytes32 messageId,
            address debtor,
            address creditor,
            uint256 amount,
            bytes32 currency
        ) = abi.decode(result, (bytes32, address, address, uint256, bytes32));
        
        require(amount > 0, "Invalid amount");
        require(debtor == msg.sender || authorizedParticipants[msg.sender], "Invalid debtor");
        
        // Verify payment amount matches
        require(msg.value >= amount, "Insufficient funds");
        
        // Store payment instruction
        payments[messageId] = PaymentInstruction({
            messageId: messageId,
            messageType: PACS_008,
            debtor: debtor,
            creditor: creditor,
            amount: amount,
            currency: currency,
            timestamp: block.timestamp,
            status: PaymentStatus.Pending,
            rawMessage: xmlMessage
        });
        
        userPayments[debtor].push(messageId);
        userPayments[creditor].push(messageId);
        
        emit PaymentInitiated(messageId, debtor, creditor, amount, currency);
    }
    
    /**
     * @notice Execute a pending payment
     * @param messageId The message ID to execute
     */
    function executePayment(bytes32 messageId) external onlyAuthorized {
        PaymentInstruction storage payment = payments[messageId];
        require(payment.amount > 0, "Payment not found");
        require(payment.status == PaymentStatus.Pending, "Invalid status");
        
        payment.status = PaymentStatus.Processing;
        emit PaymentStatusUpdated(messageId, PaymentStatus.Pending, PaymentStatus.Processing);
        
        // Transfer funds to creditor
        payable(payment.creditor).transfer(payment.amount);
        
        payment.status = PaymentStatus.Completed;
        emit PaymentStatusUpdated(messageId, PaymentStatus.Processing, PaymentStatus.Completed);
    }
    
    /**
     * @notice Reject a payment
     * @param messageId The message ID to reject
     * @param reason Rejection reason code
     */
    function rejectPayment(bytes32 messageId, string calldata reason) external onlyAuthorized {
        PaymentInstruction storage payment = payments[messageId];
        require(payment.amount > 0, "Payment not found");
        require(payment.status == PaymentStatus.Pending, "Invalid status");
        
        PaymentStatus oldStatus = payment.status;
        payment.status = PaymentStatus.Rejected;
        
        // Return funds to debtor
        payable(payment.debtor).transfer(payment.amount);
        
        emit PaymentStatusUpdated(messageId, oldStatus, PaymentStatus.Rejected);
    }
    
    /**
     * @notice Submit a status report (pacs.002)
     * @param xmlMessage The ISO 20022 XML status report
     */
    function submitStatusReport(bytes calldata xmlMessage) external onlyAuthorized {
        // Parse status report
        (bool success, bytes memory result) = ISO_PARSER.staticcall(
            abi.encode(PACS_002, xmlMessage)
        );
        require(success, "Parse failed");
        
        (bytes32 originalMessageId, uint8 newStatus) = abi.decode(result, (bytes32, uint8));
        
        PaymentInstruction storage payment = payments[originalMessageId];
        require(payment.amount > 0, "Original payment not found");
        
        PaymentStatus oldStatus = payment.status;
        payment.status = PaymentStatus(newStatus);
        
        emit PaymentStatusUpdated(originalMessageId, oldStatus, payment.status);
    }
    
    // ============ Admin Functions ============
    
    /**
     * @notice Authorize a participant
     */
    function authorizeParticipant(address participant) external onlyOwner {
        authorizedParticipants[participant] = true;
        emit ParticipantAuthorized(participant);
    }
    
    /**
     * @notice Revoke participant authorization
     */
    function revokeParticipant(address participant) external onlyOwner {
        authorizedParticipants[participant] = false;
        emit ParticipantRevoked(participant);
    }
    
    /**
     * @notice Set compliance oracle
     */
    function setComplianceOracle(address oracle) external onlyOwner {
        complianceOracle = oracle;
    }
    
    // ============ View Functions ============
    
    /**
     * @notice Get payment details
     */
    function getPayment(bytes32 messageId) external view returns (
        bytes4 messageType,
        address debtor,
        address creditor,
        uint256 amount,
        bytes32 currency,
        uint256 timestamp,
        PaymentStatus status
    ) {
        PaymentInstruction storage p = payments[messageId];
        return (
            p.messageType,
            p.debtor,
            p.creditor,
            p.amount,
            p.currency,
            p.timestamp,
            p.status
        );
    }
    
    /**
     * @notice Get user payment count
     */
    function getUserPaymentCount(address user) external view returns (uint256) {
        return userPayments[user].length;
    }
    
    /**
     * @notice Get user payment at index
     */
    function getUserPayment(address user, uint256 index) external view returns (bytes32) {
        return userPayments[user][index];
    }
    
    receive() external payable {}
}
