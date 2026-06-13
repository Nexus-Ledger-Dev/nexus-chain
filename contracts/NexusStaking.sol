// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title NexusStaking
 * @notice Staking contract for NexusChain validators
 * @dev Implements Proof of Stake with delegation support
 */
contract NexusStaking {
    // ============ Constants ============
    
    uint256 public constant MIN_STAKE = 100_000 ether;
    uint256 public constant MAX_COMMISSION = 10_000; // 100% in basis points
    uint256 public constant UNBONDING_PERIOD = 21 days;
    uint256 public constant EPOCH_DURATION = 1 days;
    
    // ============ State Variables ============
    
    struct Validator {
        address addr;
        bytes publicKey;
        uint256 stake;
        uint256 commission; // basis points
        bool active;
        uint256 uptime;
        uint256 lastEpoch;
        mapping(address => uint256) delegations;
        uint256 totalDelegated;
    }
    
    struct Delegation {
        address validator;
        uint256 amount;
        uint256 pendingRewards;
    }
    
    struct Unbonding {
        uint256 amount;
        uint256 unlockTime;
    }
    
    mapping(address => Validator) public validators;
    mapping(address => Delegation[]) public delegations;
    mapping(address => Unbonding[]) public unbondings;
    
    address[] public validatorSet;
    uint256 public totalStake;
    uint256 public currentEpoch;
    
    // ============ Events ============
    
    event ValidatorRegistered(address indexed validator, bytes publicKey, uint256 stake);
    event ValidatorUnregistered(address indexed validator);
    event Staked(address indexed delegator, address indexed validator, uint256 amount);
    event Unstaked(address indexed delegator, address indexed validator, uint256 amount);
    event RewardsClaimed(address indexed delegator, uint256 amount);
    event EpochAdvanced(uint256 indexed epoch);
    event Slashed(address indexed validator, uint256 amount, string reason);
    
    // ============ Modifiers ============
    
    modifier onlyValidator() {
        require(validators[msg.sender].active, "Not a validator");
        _;
    }
    
    // ============ External Functions ============
    
    /**
     * @notice Register as a validator
     * @param publicKey BLS public key for consensus
     * @param commission Commission rate in basis points
     */
    function registerValidator(
        bytes calldata publicKey,
        uint256 commission
    ) external payable {
        require(msg.value >= MIN_STAKE, "Insufficient stake");
        require(commission <= MAX_COMMISSION, "Commission too high");
        require(publicKey.length == 48, "Invalid public key");
        require(!validators[msg.sender].active, "Already registered");
        
        Validator storage v = validators[msg.sender];
        v.addr = msg.sender;
        v.publicKey = publicKey;
        v.stake = msg.value;
        v.commission = commission;
        v.active = true;
        v.uptime = 10000;
        v.lastEpoch = currentEpoch;
        
        validatorSet.push(msg.sender);
        totalStake += msg.value;
        
        emit ValidatorRegistered(msg.sender, publicKey, msg.value);
    }
    
    /**
     * @notice Unregister as a validator
     */
    function unregisterValidator() external onlyValidator {
        Validator storage v = validators[msg.sender];
        
        // Create unbonding entry
        unbondings[msg.sender].push(Unbonding({
            amount: v.stake + v.totalDelegated,
            unlockTime: block.timestamp + UNBONDING_PERIOD
        }));
        
        totalStake -= (v.stake + v.totalDelegated);
        v.active = false;
        
        // Remove from validator set
        for (uint256 i = 0; i < validatorSet.length; i++) {
            if (validatorSet[i] == msg.sender) {
                validatorSet[i] = validatorSet[validatorSet.length - 1];
                validatorSet.pop();
                break;
            }
        }
        
        emit ValidatorUnregistered(msg.sender);
    }
    
    /**
     * @notice Delegate stake to a validator
     * @param validator Validator address
     */
    function delegate(address validator) external payable {
        require(validators[validator].active, "Validator not active");
        require(msg.value > 0, "Amount must be > 0");
        
        Validator storage v = validators[validator];
        v.delegations[msg.sender] += msg.value;
        v.totalDelegated += msg.value;
        totalStake += msg.value;
        
        delegations[msg.sender].push(Delegation({
            validator: validator,
            amount: msg.value,
            pendingRewards: 0
        }));
        
        emit Staked(msg.sender, validator, msg.value);
    }
    
    /**
     * @notice Undelegate stake from a validator
     * @param validator Validator address
     * @param amount Amount to undelegate
     */
    function undelegate(address validator, uint256 amount) external {
        Validator storage v = validators[validator];
        require(v.delegations[msg.sender] >= amount, "Insufficient delegation");
        
        v.delegations[msg.sender] -= amount;
        v.totalDelegated -= amount;
        totalStake -= amount;
        
        // Create unbonding entry
        unbondings[msg.sender].push(Unbonding({
            amount: amount,
            unlockTime: block.timestamp + UNBONDING_PERIOD
        }));
        
        emit Unstaked(msg.sender, validator, amount);
    }
    
    /**
     * @notice Withdraw unbonded tokens
     */
    function withdrawUnbonded() external {
        uint256 total = 0;
        Unbonding[] storage userUnbondings = unbondings[msg.sender];
        
        for (uint256 i = 0; i < userUnbondings.length; ) {
            if (userUnbondings[i].unlockTime <= block.timestamp) {
                total += userUnbondings[i].amount;
                userUnbondings[i] = userUnbondings[userUnbondings.length - 1];
                userUnbondings.pop();
            } else {
                i++;
            }
        }
        
        require(total > 0, "Nothing to withdraw");
        payable(msg.sender).transfer(total);
    }
    
    /**
     * @notice Claim pending rewards
     */
    function claimRewards() external {
        uint256 total = 0;
        Delegation[] storage userDelegations = delegations[msg.sender];
        
        for (uint256 i = 0; i < userDelegations.length; i++) {
            total += userDelegations[i].pendingRewards;
            userDelegations[i].pendingRewards = 0;
        }
        
        require(total > 0, "No rewards");
        payable(msg.sender).transfer(total);
        
        emit RewardsClaimed(msg.sender, total);
    }
    
    /**
     * @notice Advance epoch and distribute rewards
     * @dev Called by consensus
     */
    function advanceEpoch() external {
        // In production, this would be called by the consensus layer
        currentEpoch++;
        
        // Distribute rewards proportionally
        uint256 rewards = address(this).balance / 100; // 1% of balance as rewards
        
        for (uint256 i = 0; i < validatorSet.length; i++) {
            address vAddr = validatorSet[i];
            Validator storage v = validators[vAddr];
            
            if (v.active && v.stake + v.totalDelegated > 0) {
                uint256 share = rewards * (v.stake + v.totalDelegated) / totalStake;
                uint256 commission = share * v.commission / MAX_COMMISSION;
                
                // Validator gets commission
                v.stake += commission;
                
                // Delegators share the rest proportionally
                // (simplified - in production, track per-delegator)
            }
        }
        
        emit EpochAdvanced(currentEpoch);
    }
    
    /**
     * @notice Slash a validator
     * @param validator Validator to slash
     * @param amount Amount to slash
     * @param reason Reason for slashing
     */
    function slash(
        address validator,
        uint256 amount,
        string calldata reason
    ) external {
        // In production, this would be called by consensus
        Validator storage v = validators[validator];
        require(v.active, "Not active");
        
        uint256 slashAmount = amount > v.stake ? v.stake : amount;
        v.stake -= slashAmount;
        totalStake -= slashAmount;
        
        emit Slashed(validator, slashAmount, reason);
    }
    
    // ============ View Functions ============
    
    /**
     * @notice Get validator count
     */
    function validatorCount() external view returns (uint256) {
        return validatorSet.length;
    }
    
    /**
     * @notice Get validator info
     */
    function getValidator(address addr) external view returns (
        bytes memory publicKey,
        uint256 stake,
        uint256 commission,
        bool active,
        uint256 uptime,
        uint256 totalDelegated
    ) {
        Validator storage v = validators[addr];
        return (
            v.publicKey,
            v.stake,
            v.commission,
            v.active,
            v.uptime,
            v.totalDelegated
        );
    }
    
    /**
     * @notice Get delegation amount
     */
    function getDelegation(
        address delegator,
        address validator
    ) external view returns (uint256) {
        return validators[validator].delegations[delegator];
    }
    
    receive() external payable {}
}
