use alloy_primitives::{address, Address};
use alloy_sol_types::sol;

pub const CONTRACT_ADDRESS: Address = address!("0x0000000000000000000000000000000000002000");

sol! {
    #[sol(rpc)]
    contract ValidatorManager {
        #[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
        struct Secp256k1Key {
            uint256 x;
            uint256 y;
        }

        #[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
        struct ValidatorInfo {
            Secp256k1Key validatorKey;
            uint64 power;
        }

        event ValidatorRegistered(bytes32 indexed validatorKeyId, Secp256k1Key validatorKey, uint64 power);
        event ValidatorUnregistered(bytes32 indexed validatorKeyId, Secp256k1Key validatorKey);
        event ValidatorPowerUpdated(bytes32 indexed validatorKeyId, Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower);

        error ValidatorAlreadyExists();
        error ValidatorDoesNotExist();
        error InvalidPower();
        error InvalidKey();
        error TotalPowerOverflow();

        function addAndRemove(ValidatorInfo[] addValidators, Secp256k1Key[] removeValidatorKeys) external;
        function registerSet(ValidatorInfo[] addValidators) external;
        function register(Secp256k1Key validatorKey, uint64 power) external;
        function unregisterSet(Secp256k1Key[] validatorKeys) external;
        function unregister(Secp256k1Key validatorKey) external;
        function updatePower(Secp256k1Key validatorKey, uint64 newPower) external;

        function getValidator(Secp256k1Key validatorKey) external view returns (ValidatorInfo info);
        function getValidators() external view returns (ValidatorInfo[] validators);
        function getValidatorCount() external view returns (uint256 count);
        function isValidator(Secp256k1Key validatorKey) external view returns (bool isRegistered);
        function getValidatorKeys() external view returns (Secp256k1Key[] validatorKeys);
        function getTotalPower() external view returns (uint64 totalPower);
    }
}
