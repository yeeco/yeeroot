use primitives::{ed25519, sr25519, Pair};
use yee_runtime::{
	AccountId, GenesisConfig, ConsensusConfig, TimestampConfig, BalancesConfig,
	IndicesConfig,
    PowConfig, ShardingConfig,
};
use substrate_service;

use ed25519::Public as AuthorityId;

// Note this is the URL for the telemetry server
//const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = substrate_service::ChainSpec<GenesisConfig>;

/// The chain specification option. This is expected to come in from the CLI and
/// is little more than one of a number of alternatives which can easily be converted
/// from a string (`--chain=...`) into a `ChainSpec`.
#[derive(Clone, Debug)]
pub enum Alternative {
	/// Whatever the current runtime is, with just Alice as an auth.
	Development,
	/// Whatever the current runtime is, with simple Alice/Bob auths.
	LocalTestnet,
    /// Proof-of-Concept chain with prebuilt runtime.
    POCTestnet,
}

fn authority_key(s: &str) -> AuthorityId {
	ed25519::Pair::from_string(&format!("//{}", s), None)
		.expect("static values are valid; qed")
		.public()
}

fn account_key(s: &str) -> AccountId {
	sr25519::Pair::from_string(&format!("//{}", s), None)
		.expect("static values are valid; qed")
		.public()
}

impl Alternative {
	/// Get an actual chain config from one of the alternatives.
	pub(crate) fn load(self) -> Result<ChainSpec, String> {
		Ok(match self {
			Alternative::Development => ChainSpec::from_genesis(
				"Development",
				"dev",
				|| testnet_genesis(vec![
					authority_key("Alice")
				], vec![
					account_key("Alice")
				],
				),
				vec![],
				None,
				None,
				None,
				None
			),
			Alternative::LocalTestnet => ChainSpec::from_genesis(
				"Local Testnet",
				"local_testnet",
				|| testnet_genesis(vec![
					authority_key("Alice"),
					authority_key("Bob"),
				], vec![
					account_key("Alice"),
					account_key("Bob"),
					account_key("Charlie"),
					account_key("Dave"),
					account_key("Eve"),
					account_key("Ferdie"),
				],
				),
				vec![],
				None,
				None,
				None,
				None
			),
            Alternative::POCTestnet => ChainSpec::from_genesis(
                "POC Testnet",
                "poc_testnet",
                || poc_testnet_genesis(vec![
                ]),
                vec![],
                None,
                None,
                None,
                None,
            ),
		})
	}

	pub(crate) fn from(s: &str) -> Option<Self> {
		match s {
			"dev" => Some(Alternative::Development),
            "local" => Some(Alternative::LocalTestnet),
            "" | "poc" => Some(Alternative::POCTestnet),
			_ => None,
		}
	}
}

fn testnet_genesis(initial_authorities: Vec<AuthorityId>, endowed_accounts: Vec<AccountId>) -> GenesisConfig {
    let code = include_bytes!("../runtime/wasm/target/wasm32-unknown-unknown/release/yee_runtime_wasm.compact.wasm").to_vec();
    testnet_template_genesis(initial_authorities, endowed_accounts, code)
}

fn poc_testnet_genesis(endowed_accounts: Vec<AccountId>) -> GenesisConfig {
    let code = include_bytes!("../prebuilt/yee_runtime/poc_testnet.wasm").to_vec();
    testnet_template_genesis(vec![], endowed_accounts, code)
}

fn testnet_template_genesis(initial_authorities: Vec<AuthorityId>, endowed_accounts: Vec<AccountId>, code: Vec<u8>) -> GenesisConfig {
	GenesisConfig {
		consensus: Some(ConsensusConfig {
			code,
			authorities: initial_authorities.clone(),
		}),
		system: None,
		timestamp: Some(TimestampConfig {
			minimum_period: 0, // 10 second block time.
		}),
        pow: Some(PowConfig {
            genesis_difficulty: primitives::U256::from(0x00003fff) << 224,
            difficulty_adj: 60_u64.into(),
            target_block_time: 15_u64.into(),
        }),
		indices: Some(IndicesConfig {
			ids: endowed_accounts.clone(),
		}),
		balances: Some(BalancesConfig {
			transaction_base_fee: 1,
			transaction_byte_fee: 0,
			existential_deposit: 500,
			transfer_fee: 0,
			creation_fee: 0,
			balances: endowed_accounts.iter().cloned().map(|k|(k, 1 << 60)).collect(),
			vesting: vec![],
		}),
        sharding: Some(ShardingConfig {
            _genesis_phantom_data: Default::default(),
            sharding_count: 4,
        }),
	}
}
