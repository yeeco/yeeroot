use jsonrpc_derive::rpc;
use primitives::{U256, sr25519, storage::{StorageKey, StorageData}};
use crate::rpc::futures::{Future, Stream};
use crate::Config;
use crate::client::RpcClient;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use parity_codec::{KeyedVec, Codec, Decode, Encode, Input, Compact};
use sr_io::blake2_256;
use num_bigint::BigUint;
use yee_runtime::{AccountId, BlockNumber};
use yee_sharding_primitives::utils::shard_num_for_bytes;
use crate::errors;
use jsonrpc_core::{BoxFuture, Error, ErrorCode};
use crate::rpc::{self, futures::future::{self, FutureResult}};
use yee_serde_hex::Hex;
use yee_primitives::{Address, AddressCodec, Hrp};
use serde_json::Value;
use hex;
use yee_sr_primitives::SHARD_CODE_SIZE;
use serde_json::map::Entry::Vacant;
use serde::export::PhantomData;
use crate::work_manager::{WorkManager, Work};
use runtime_primitives::traits::{BlakeTwo256, Hash as HashT};
use std::sync::Arc;
use yee_serde_hex::SerdeHex;
use parking_lot::RwLock;
use yee_consensus_pow_primitives::PowTarget;
use yee_consensus_pow::ExtraData;
use tokio::timer::Interval;
use tokio::runtime::{Runtime, TaskExecutor};
use std::time::{Instant, Duration};
use log::{info, debug};
use std::ptr::read_unaligned;
use parity_codec::alloc::collections::HashMap;

#[rpc]
pub trait PowApi<Hash> {
    #[rpc(name = "get_work")]
    fn get_work(&self) -> errors::Result<Job<Hash>>;

    #[rpc(name = "submit_work")]
    fn submit_work(&self, data: String) -> BoxFuture<()>;
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Job<Hash> {
    pub merkle_root: Hash,
    pub extra_data: ExtraData,
    pub target: PowTarget,
    pub shard_count: u16,
    pub shard_block_number: HashMap<u16, BlockNumber>,
}

#[derive(Default, Debug)]
struct SubmitJob<Hash> {
    merkle_root: Hash,
    extra_data: ExtraData,
    nonce: u64,
    nonce_target: U256,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct SubmitResponse {
    pub reject_reason: Vec<u8>,
}

pub struct Pow<WM: WorkManager> where {
    work_manager: Arc<RwLock<WM>>
}

impl<WM: WorkManager> Pow<WM> where
{
    pub fn new(wm: Arc<RwLock<WM>>) -> Self {
        Self {
            work_manager: wm,
        }
    }
}

impl<WM> PowApi<<WM::Hashing as HashT>::Output> for Pow<WM> where
    WM: WorkManager<Number=BlockNumber> + Send + Sync + 'static,
    <WM::Hashing as HashT>::Output: Decode + Encode,
{
    fn get_work(&self) -> errors::Result<Job<<WM::Hashing as HashT>::Output>> {
        let work = self.work_manager.read().get_work()?;
        info!("pow/mod.rs -> get_work: {:?}", work);

        let root = work.merkle_root;
        if let Ok(work) = self.work_manager.read().get_work_by_merkle(root) {
            debug!("work exist {:?}", work);
        }

        let job = Job {
            merkle_root: work.merkle_root,
            extra_data: work.extra_data,
            target: work.target,
            shard_count: work.shard_count,
            shard_block_number: work.shard_block_number,
        };

        Ok(job)
    }

    fn submit_work(&self, data: String) -> BoxFuture<()> {
        info!("pow/mod.rs -> submit_work, raw-data: {}", data);

        let bytes = match hex::decode(data) {
            Ok(bytes) => bytes,
            Err(_) => return Box::new(
                future::err(errors::Error::from(errors::ErrorKind::SumbitWorkError("submit data format error".to_string())).into()))
        };
        let job = match self.decode_submit_work(bytes) {
            Some(job) => job,
            None => return Box::new(
                future::err(errors::Error::from(errors::ErrorKind::SumbitWorkError("submit data format error".to_string())).into()))
        };
        let mut work = match self.work_manager.read().get_work_by_merkle(job.merkle_root) {
            Ok(work) => work,
            Err(e) => return Box::new(
                future::err(errors::Error::from(errors::ErrorKind::SumbitWorkError("not found work by merkle root".to_string())).into())),
        };

        work.extra_data = job.extra_data;
        work.nonce = Some(job.nonce);
        work.nonce_target = Some(job.nonce_target);

        Box::new(self.work_manager.write().submit_work_future(work).map_err(|e| {
            errors::Error::from(errors::ErrorKind::SumbitWorkError(format!("{}", e))).into()
        }))
    }
}

impl<WM> Pow<WM> where
    WM: WorkManager + Send + Sync + 'static,
    <WM::Hashing as HashT>::Output: Decode + Encode,
{
    fn decode_submit_work(&self, input: Vec<u8>) -> Option<SubmitJob<<WM::Hashing as HashT>::Output>> {
        if input.len() < 76 {
            return None;
        }
        let bytes = input.as_slice();
        let mut root_data = &bytes[..32];
        let merkle_root: <WM::Hashing as HashT>::Output = match Decode::decode(&mut root_data) {
            Some(r) => r,
            None => return None
        };
        let nonce = &bytes[32..40];
        let mut buf = [0u8; 8];
        for i in 0..8 {
            buf[i] = nonce[7 - i];
        }
        let nonce = match u64::from_bytes(&buf[..]) {
            Ok(n) => n,
            Err(_e) => return None
        };

        let check = BlakeTwo256::hash(&bytes[40..76]);
        let check = &check[..4];

        let mut extra_data = [0u8; 40];
        for i in 0..36 {
            extra_data[i] = bytes[40 + i];
        }
        // extra_data.copy_from_slice(&bytes[40..76]);
        for i in 0..4 {
            extra_data[i + 36] = check[i];
        }
        let extra_data = ExtraData::from(extra_data);

        let source = (merkle_root.clone(), nonce, extra_data.clone());
        let buf = source.encode();
        debug!("submit work,data: {:?}", hex::encode(buf));
        let source_hash = WM::Hashing::hash_of(&source);
        let nonce_target = PowTarget::from(source_hash.as_ref());
        Some(SubmitJob { merkle_root, extra_data, nonce, nonce_target })
    }
}
