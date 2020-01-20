#![cfg_attr(not(feature = "std"), no_std)]

use parity_codec::{Encode, Decode, Codec, Input, Compact};
use rstd::prelude::*;
use substrate_primitives::{Blake2Hasher, Hasher, H256};
use substrate_sr_primitives::generic::Era;

#[derive(PartialEq, Eq, Clone)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct RelayParams<Hash> where
    Hash: Codec + Clone,
{
    number: Compact<u64>,
    hash: Hash,
    block_hash: Hash,
    parent_hash: Hash,

    relay_type: RelayTypes,

    origin: Vec<u8>,
    // origin_extrinsic: OriginExtrinsic<AccountId, Balance>,
}

const MIN_RELAY_SIZE: usize = 2 + 32 + 32 + 64;

impl<Hash> RelayParams<Hash> where
    Hash: Codec + Clone
{
    pub fn relay_type(&self) -> RelayTypes {
        self.relay_type.clone()
    }

    pub fn origin(&self) -> Vec<u8> {
        self.origin.clone()
    }

    pub fn number(&self) -> Compact<u64> {
        self.number.clone()
    }

    pub fn hash(&self) -> Hash {
        self.hash.clone()
    }

    pub fn block_hash(&self) -> Hash {
        self.block_hash.clone()
    }

    pub fn parent_hash(&self) -> Hash {
        self.parent_hash.clone()
    }

    /// decode from input
    pub fn decode(input: Vec<u8>) -> Option<Self> {
        let mut input = input.as_slice();
        if input.len() <= MIN_RELAY_SIZE {
            return None;
        }
        // length
        let _len: Vec<()> = match Decode::decode(&mut input) {
            Some(len) => len,
            None => return None
        };
        // version
        let version = match input.read_byte() {
            Some(v) => v,
            None => return None
        };
        // is signed
        let is_signed = version & 0b1000_0000 != 0;
        let version = version & 0b0111_1111;
        // has signed or version not satisfy
        if is_signed || version != 1u8 {
            return None;
        }
        // module
        let _module: u8 = match input.read_byte() {
            Some(m) => m,
            None => return None
        };
        // function
        let _func: u8 = match input.read_byte() {
            Some(f) => f,
            None => return None
        };
        // relay type
        let relay_type: RelayTypes = match Decode::decode(&mut input) {
            Some(t) => t,
            None => return None
        };
        // origin transfer
        let origin: Vec<u8> = match Decode::decode(&mut input) {
            Some(ot) => ot,
            None => return None
        };
        // which block's number the origin transfer in
        let number: Compact<u64> = match Decode::decode(&mut input) {
            Some(h) => h,
            None => return None
        };
        // block hash
        let block_hash: Hash = match Decode::decode(&mut input) {
            Some(h) => h,
            None => return None
        };
        // which block's parent hash the origin transfer in
        let parent_hash: Hash = match Decode::decode(&mut input) {
            Some(h) => h,
            None => return None
        };
        let hash = Decode::decode(&mut Blake2Hasher::hash(origin.as_slice()).encode().as_slice()).unwrap();
        Some(Self { number, hash, block_hash, parent_hash, relay_type, origin })
    }
}

#[derive(PartialEq, Eq, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum RelayTypes {
    Balance,
    Assets,
}

//pub enum OriginExtrinsic<AccountId, Balance> where
//    AccountId: Decode + Clone,
//    Balance: Decode + Clone,
//{
//    Balance(OriginBalance<AccountId, Balance>),
//    Asset(OriginAsset<AccountId, Balance>),
//}
//
//impl<AccountId, Balance> OriginExtrinsic<AccountId, Balance> where
//    AccountId: Decode + Clone,
//    Balance: Decode + Clone,
//{
//    pub fn from(&self) -> AccountId {
//        match self {
//            OriginExtrinsic::Balance(origin) => origin.from(),
//            OriginExtrinsic::Asset(origin) => origin.from(),
//        }
//    }
//
//    pub fn to(&self) -> AccountId {
//        match self {
//            OriginExtrinsic::Balance(origin) => origin.to(),
//            OriginExtrinsic::Asset(origin) => origin.to(),
//        }
//    }
//
//    pub fn amount(&self) -> Balance {
//        match self {
//            OriginExtrinsic::Balance(origin) => origin.amount(),
//            OriginExtrinsic::Asset(origin) => origin.amount(),
//        }
//    }
//
//    pub fn asset_id(&self) -> Option<u32> {
//        match self {
//            OriginExtrinsic::Asset(origin) => origin.asset_id(),
//            _ => None
//        }
//    }
//}
//struct OriginBalance<AccountId, Balance> where AccountId: Clone, Balance: Clone {
//    sender: AccountId,
//    signature: Vec<u8>,
//    index: Compact<u64>,
//    era: Era,
//    dest: AccountId,
//    amount: Balance,
//}
//
//impl<AccountId, Balance> OriginTrait<AccountId, Balance> for OriginBalance<AccountId, Balance> where
//    AccountId: Decode + Clone,
//    Balance: Decode + Clone,
//{
//    fn from(&self) -> AccountId {
//        self.sender.clone()
//    }
//
//    fn to(&self) -> AccountId {
//        self.dest.clone()
//    }
//
//    fn amount(&self) -> Balance {
//        self.amount.clone()
//    }
//}

pub trait OriginTrait<AccountId, Balance> where
    AccountId: Codec + Clone,
    Balance: Codec + Clone,
{
    fn from(&self) -> AccountId;
    fn to(&self) -> AccountId;
    fn amount(&self) -> Balance;

    fn asset_id(&self) -> Option<u32>;
}

/// OriginAsset for asset transfer
pub struct OriginExtrinsic<AccountId, Balance> where
    AccountId: Codec + Clone + Default,
    Balance: Codec + Clone,
{
    id: Option<u32>,
    sender: AccountId,
    signature: Vec<u8>,
    index: Compact<u64>,
    era: Era,
    dest: AccountId,
    amount: Balance,
}

impl<AccountId, Balance> OriginExtrinsic<AccountId, Balance> where
    AccountId: Codec + Clone + Default,
    Balance: Codec + Clone,
{
    pub fn decode(relay_type: RelayTypes, input: Vec<u8>) -> Option<OriginExtrinsic<AccountId, Balance>> {
        let mut input = input.as_slice();
        if input.len() < 64 + 1 + 1 {
            return None;
        }
        // length
        let _len: Vec<()> = match Decode::decode(&mut input) {
            Some(len) => len,
            None => return None
        };
        // version
        let version = match input.read_byte() {
            Some(v) => v,
            None => return None
        };
        // is signed
        let is_signed = version & 0b1000_0000 != 0;
        let version = version & 0b0111_1111;
        if version != 1u8 {
            return None;
        }

        let (sender, signature, index, era): (AccountId, Vec<u8>, Compact<u64>, Era) = if is_signed {
            // sender type
            let _type = match input.read_byte() {
                Some(a_t) => a_t,
                None => return None
            };
            // sender
            let sender = match Decode::decode(&mut input) {
                Some(s) => s,
                None => return None
            };
            if input.len() < 64 {
                return None;
            }
            // signature
            let signature = input[..64].to_vec();
            input = &input[64..];
            // index
            let index = match Decode::decode(&mut input) {
                Some(i) => i,
                None => return None
            };
            if input.len() < 1 {
                return None;
            }
            // era
            let era = if input[0] != 0u8 {
                match Decode::decode(&mut input) {
                    Some(e) => e,
                    None => return None
                }
            } else {
                input = &input[1..];
                Era::Immortal
            };
            (sender, signature, index, era)
        } else {
            (AccountId::default(), Vec::new(), Compact(0u64), Era::Immortal)
        };

        if input.len() < 2 + 32 + 1 {
            return None;
        }
        // module
        let _module: u8 = match input.read_byte() {
            Some(m) => m,
            None => return None
        };
        // function
        let _func: u8 = match input.read_byte() {
            Some(f) => f,
            None => return None
        };
        // AssetId
        let mut id: Compact<u32> = Compact(0u32);
        if relay_type == RelayTypes::Assets {
            id = match Decode::decode(&mut input) {
                Some(id) => id,
                None => return None
            };
        }
        // dest AccountId type
        let _type: u8 = match input.read_byte() {
            Some(t) => t,
            None => return None
        };
        // dest AccountId
        let dest: AccountId = match Decode::decode(&mut input) {
            Some(addr) => addr,
            None => return None
        };
        // amount
        let amount: Balance = match Decode::decode(&mut input) {
            Some(a) => {
                let a_c: Compact<u128> = a;
                let buf = a_c.0.encode();
                match Decode::decode(&mut buf.as_slice()) {
                    Some(am) => am,
                    None => return None
                }
            }
            None => return None
        };
        if relay_type == RelayTypes::Assets {
            Some(Self { id: Some(id.0), sender, signature, index, era, dest, amount })
        } else if relay_type == RelayTypes::Balance {
            Some(Self { id: None, sender, signature, index, era, dest, amount })
        } else {
            None
        }
    }

    pub fn from(&self) -> AccountId {
        self.sender.clone()
    }

    pub fn to(&self) -> AccountId {
        self.dest.clone()
    }

    pub fn amount(&self) -> Balance {
        self.amount.clone()
    }

    pub fn asset_id(&self) -> Option<u32> {
        self.id.clone()
    }
}
