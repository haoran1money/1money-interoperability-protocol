use alloy_primitives::{address, Address};
use alloy_sol_types::sol;

pub const CONTRACT_ADDRESS: Address = address!("0x0000000000000000000000000000000000002000");

sol! {
    #[sol(rpc, abi)]
    #[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
    contract ValidatorManager {
        struct Secp256k1Key {
            uint256 x;
            uint256 y;
        }

        struct ValidatorInfo {
            Secp256k1Key validatorKey;
            uint64 power;
        }

        struct ValidatorRegistration {
            bytes publicKey;
            uint64 power;
        }

        event ValidatorRegistered(address indexed validatorAddress, Secp256k1Key validatorKey, uint64 power);
        event ValidatorUnregistered(address indexed validatorAddress, Secp256k1Key validatorKey);
        event ValidatorPowerUpdated(address indexed validatorAddress, Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower);

        error ValidatorAlreadyExists();
        error ValidatorDoesNotExist();
        error InvalidPower();
        error InvalidKey();
        error TotalPowerOverflow();
        error InvalidTotalPowerAccounting();
        error InvalidPublicKeyLength();
        error InvalidPublicKeyFormat();
        error InvalidPublicKeyCoordinates();

        function updateValidatorSet(ValidatorRegistration[] calldata addValidators, address[] calldata removeValidatorAddresses) external;
        function registerSet(ValidatorRegistration[] calldata registrations) external;
        function register(bytes calldata validatorPublicKey, uint64 power) external;
        function unregisterSet(address[] calldata validatorAddresses) external;
        function unregister(address validatorAddress) external;
        function updatePower(address validatorAddress, uint64 newPower) external;

        function getValidator(address validatorAddress) external view returns (ValidatorInfo info);
        function getValidators() external view returns (ValidatorInfo[] validators);
        function getValidatorAddresses() external view returns (address[] addresses);
        function getValidatorCount() external view returns (uint256 count);
        function isValidator(address validatorAddress) external view returns (bool contains);
        function getTotalPower() external view returns (uint64 totalPower);

        function _validatorAddress(Secp256k1Key memory validatorKey) external pure returns (address);
        function _secp256k1KeyFromBytes(bytes calldata validatorPublicKey) external pure returns (Secp256k1Key memory);
    }
}

impl ValidatorManager::Secp256k1Key {
    pub fn verifying_key(&self) -> alloy_signer::k256::ecdsa::VerifyingKey {
        let x_bytes: [u8; 32] = self.x.to_be_bytes();
        let y_bytes: [u8; 32] = self.y.to_be_bytes();

        let mut encoded = [0u8; 65];
        encoded[0] = 0x04; // Uncompressed point prefix
        encoded[1..33].copy_from_slice(&x_bytes);
        encoded[33..65].copy_from_slice(&y_bytes);

        alloy_signer::k256::ecdsa::VerifyingKey::from_sec1_bytes(&encoded)
            .expect("valid verifying key")
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        let verifying_key = self.verifying_key();
        let public_key = verifying_key.to_encoded_point(true);
        public_key.as_bytes().to_vec()
    }

    pub fn address(&self) -> Address {
        let verifying_key = self.verifying_key();
        let public_key = verifying_key.to_encoded_point(false);
        let public_key_bytes = public_key.as_bytes();
        let hash = alloy_primitives::keccak256(&public_key_bytes[1..]);
        Address::from_slice(&hash[12..])
    }
}
