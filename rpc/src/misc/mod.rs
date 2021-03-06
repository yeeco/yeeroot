use std::sync::Arc;
use std::hash::{Hasher, Hash};
use parity_codec::{Encode, Decode};
use jsonrpc_derive::rpc;
use jsonrpc_core::BoxFuture;
use crate::{errors, Config};
use jsonrpc_core::futures::future::{self, Future, IntoFuture};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde_json::Value;
use std::marker::PhantomData;
use yee_primitives::RecommitRelay;
use futures::sync::mpsc;
use parking_lot::RwLock;
use crfg::CrfgStateProvider;
use std::time::Duration;
use substrate_primitives::{Bytes, H256, Blake2Hasher};
use transaction_pool::txpool::{Pool, ChainApi as PoolChainApi};
use yee_foreign_network::{SyncProvider, NetworkState};
use runtime_primitives::traits::{Block as BlockT, NumberFor};
use runtime_primitives::generic::BlockId;
use std::fmt::Debug;
use client::Client;
use crate::misc::types::ForeignStatus;
use parity_codec::alloc::collections::HashMap;

#[rpc]
pub trait MiscApi<Hash, Number> {
	#[rpc(name = "author_recommitRelay")]
	fn recommit_relay_extrinsic(&self, hash: Hash, index: usize) -> errors::Result<()>;

	#[rpc(name = "author_waitingExtrinsics")]
	fn waiting_extrinsics(&self) -> errors::Result<Vec<Bytes>>;

	#[rpc(name = "system_foreignNetworkState")]
	fn foreign_network_state(&self) -> errors::Result<NetworkState>;

	#[rpc(name = "system_foreignStatus")]
	fn foreign_status(&self) -> errors::Result<HashMap<u16, Option<ForeignStatus<Hash, Number>>>>;

	#[rpc(name = "system_config")]
	fn system_config(&self) -> errors::Result<Config>;

	#[rpc(name = "system_syncState")]
	fn sync_state(&self) -> errors::Result<HashMap<u16, types::CrfgState<Hash, Number>>>;

	#[rpc(name = "system_syncInspect")]
	fn sync_inspect(&self) -> errors::Result<()>;

	#[rpc(name = "crfg_state")]
	fn crfg_state(&self) -> errors::Result<Option<types::CrfgState<Hash, Number>>>;

	#[rpc(name = "chain_getRelayProof")]
	fn get_relay_proof(&self, hash: Option<Hash>) -> errors::Result<Option<Bytes>>;



}

pub struct Misc<P: PoolChainApi, B: BlockT, H, Backend, E, RA> {
	recommit_relay_sender: Arc<RwLock<Option<mpsc::UnboundedSender<RecommitRelay<B::Hash>>>>>,
	crfg_state_provider: Arc<RwLock<Option<Arc<CrfgStateProvider<B::Hash, NumberFor<B>>>>>>,
	import_crfg_state_providers: Arc<RwLock<HashMap<u16, Arc<dyn CrfgStateProvider<B::Hash, NumberFor<B>>>>>>,
	pool: Arc<Pool<P>>,
	network: Arc<network::SyncProvider<B>>,
	foreign_network: Arc<RwLock<Option<Arc<dyn SyncProvider<B, H>>>>>,
	client: Arc<Client<Backend, E, B, RA>>,
	config: Arc<Config>,
}

impl<P: PoolChainApi, B, H, Backend, E, RA> Misc<P, B, H, Backend, E, RA>
where
	P: PoolChainApi,
	B: BlockT<Hash=H256>,
	Backend: client::backend::Backend<B, Blake2Hasher> + Send + Sync + 'static,
	E: client::CallExecutor<B, Blake2Hasher> + Send + Sync + 'static,
	RA: Send + Sync + 'static
{
	pub fn new(
		recommit_relay_sender: Arc<RwLock<Option<mpsc::UnboundedSender<RecommitRelay<B::Hash>>>>>,
		crfg_state_provider: Arc<RwLock<Option<Arc<dyn CrfgStateProvider<B::Hash, NumberFor<B>>>>>>,
		import_crfg_state_providers: Arc<RwLock<HashMap<u16, Arc<dyn CrfgStateProvider<B::Hash, NumberFor<B>>>>>>,
		pool: Arc<Pool<P>>,
		network: Arc<network::SyncProvider<B>>,
		foreign_network: Arc<RwLock<Option<Arc<dyn SyncProvider<B, H>>>>>,
		client: Arc<Client<Backend, E, B, RA>>,
		config: Arc<Config>,
	) -> Self {
		Self {
			recommit_relay_sender,
			crfg_state_provider,
			import_crfg_state_providers,
			pool,
			network,
			foreign_network,
			client,
			config,
		}
	}

	fn unwrap_or_best(&self, hash: Option<B::Hash>) -> errors::Result<B::Hash> {
		Ok(match hash.into() {
			None => self.client.info()?.chain.best_hash,
			Some(hash) => hash,
		})
	}
}

impl<P, B, H, Backend, E, RA> MiscApi<B::Hash, NumberFor<B>> for Misc<P, B, H, Backend, E, RA> where
	B: BlockT<Hash=H256> + Send + Clone + Sync + 'static,
	P: PoolChainApi + Sync + Send + 'static,
	H: Eq + Hash + Debug + Clone + Sync + Send + 'static,
	Backend: client::backend::Backend<B, Blake2Hasher> + Send + Sync + 'static,
	E: client::CallExecutor<B, Blake2Hasher> + Send + Sync + 'static,
	RA: Send + Sync + 'static
{
	fn recommit_relay_extrinsic(&self, hash: B::Hash, index: usize) -> errors::Result<()> {
		let recommit_param = RecommitRelay {
			hash,
			index,
        };
		if let Some(sender) =  self.recommit_relay_sender.write().as_ref() {
			sender.unbounded_send(recommit_param);
			Ok(())
		} else {
			Err(errors::Error::from(errors::ErrorKind::RecommitFailed).into())
		}
	}

	fn waiting_extrinsics(&self) -> errors::Result<Vec<Bytes>> {
		Ok(self.pool.futures().map(|tx| tx.data.encode().into()).collect())
	}

	fn foreign_network_state(&self) -> errors::Result<NetworkState> {
		let foreign_network = self.foreign_network.read();
		let foreign_network = foreign_network.as_ref().ok_or(errors::Error::from(errors::ErrorKind::NotReady))?;
		Ok(foreign_network.network_state())
	}

	fn foreign_status(&self) -> errors::Result<HashMap<u16, Option<ForeignStatus<B::Hash, NumberFor<B>>>>> {
		let foreign_network = self.foreign_network.read();
		let foreign_network = foreign_network.as_ref().ok_or(errors::Error::from(errors::ErrorKind::NotReady))?;
		let client_info = foreign_network.client_info();
		let network_state = foreign_network.network_state();
		let main_peer_count = self.network.network_state().connected_peers.len() as u32;

		let mut peer_count_map: HashMap<u16, u32> = HashMap::new();
		peer_count_map.insert(self.config.shard_num, main_peer_count);
		for (_peer_id, peer) in &network_state.connected_peers {
			match peer.shard_num {
				Some(shard_num) => {
					let count = peer_count_map.entry(shard_num).or_insert(0);
					*count = *count + 1;
				}
				None => {}
			}
		}

		let status = client_info.into_iter().map(|(k, v)|{
			let status = v.map(|v|{
				ForeignStatus {
					peer_count: *peer_count_map.get(&k).unwrap_or(&0u32),
					best_number: v.chain.best_number,
					best_hash: v.chain.best_hash,
					finalized_number: v.chain.finalized_number,
					finalized_hash: v.chain.finalized_hash,
				}
			});
			(k, status)
		}).collect::<HashMap<_, _>>();

		Ok(status)
	}

	fn system_config(&self) -> errors::Result<Config> {

		let config = (*self.config).clone();
		Ok(config)
	}

	fn crfg_state(&self) -> errors::Result<Option<types::CrfgState<B::Hash, NumberFor<B>>>> {
		let state = self.crfg_state_provider.read().as_ref().cloned().map(|x|x.crfg_state().into());
		Ok(state)
	}

	fn get_relay_proof(&self, hash: Option<B::Hash>) -> errors::Result<Option<Bytes>> {
		let hash = self.unwrap_or_best(hash)?;
		let proof = self.client.proof(&BlockId::Hash(hash))?;
		let proof = proof.map(|x|Bytes(x));
		Ok(proof)
	}

	fn sync_state(&self) -> errors::Result<HashMap<u16, types::CrfgState<B::Hash, NumberFor<B>>>> {
		let state = self.import_crfg_state_providers.read().iter().map(|(k, v)|{
			(*k, v.crfg_state().into())
		}).collect::<HashMap<_, _>>();
		Ok(state)
	}

	fn sync_inspect(&self) -> errors::Result<()> {

		self.network.inspect();
		let foreign_network = self.foreign_network.read();
		foreign_network.as_ref().map(|x|x.inspect());

		Ok(())
	}
}

mod types {
	use std::sync::Arc;
	use yee_runtime::{AuthorityId, BlockNumber};
	use std::time::Duration;
	use parity_codec::alloc::collections::HashMap;
	use serde::Serialize;
	use yee_serde_hex::SerdeHex;
	use substrate_primitives::crypto::Pair;

	#[derive(Serialize, Eq, PartialEq, Hash)]
	pub struct Public(#[serde(with = "SerdeHex")] Vec<u8>);

	#[derive(Serialize)]
	pub struct CrfgState<H, N> {
		pub config: Option<Config>,
		pub set_id: u64,
		pub voters: VoterSet,
		pub set_status: Option<VoterSetState<H, N>>,
		pub pending_skip: Vec<(H, N, N)>,
	}

	#[derive(Serialize)]
	pub struct Config {
		pub gossip_duration: Duration,
		pub justification_period: u64,
		pub local_key_public: Option<Public>,
		pub local_next_key_public: Option<Public>,
		pub name: Option<String>,
	}

	#[derive(Serialize)]
	pub struct SyncState<H, N> {
		pub pending_skip: Vec<(H, N, N)>,
	}

	#[derive(Serialize)]
	pub struct VoterInfo {
		canon_idx: usize,
		weight: u64,
	}

	#[derive(Serialize)]
	pub struct VoterSet {
		weights: HashMap<Public, VoterInfo>,
		voters: Vec<(Public, u64)>,
		threshold: u64,
	}

	#[derive(Serialize)]
	pub enum VoterSetState<H, N> {
		Paused(u64, RoundState<H, N>),
		Live(u64, RoundState<H, N>),
	}

	#[derive(Serialize)]
	pub struct RoundState<H, N> {
		pub prevote_ghost: Option<(H, N)>,
		pub finalized: Option<(H, N)>,
		pub estimate: Option<(H, N)>,
		pub completable: bool,
	}

	#[derive(Serialize)]
	pub struct ForeignStatus<H, N> {
		pub peer_count: u32,
		pub best_number: N,
		pub best_hash: H,
		pub finalized_number: N,
		pub finalized_hash: H,
	}

	impl<H, N> From<crfg::CrfgState<H, N>> for CrfgState<H, N> where
		H: Clone,
		N: Clone,
	{
		fn from(t: crfg::CrfgState<H, N>) -> CrfgState<H, N> {
			CrfgState {
				config: t.config.map(Into::into),
				set_id: t.set_id,
				voters: t.voters.into(),
				set_status: t.set_status.map(Into::into),
				pending_skip: t.pending_skip,
			}
		}
	}

	impl From<crfg::Config> for Config {
		fn from(t: crfg::Config) -> Config {
			Config {
				gossip_duration: t.gossip_duration,
				justification_period: t.justification_period,
				local_key_public: t.local_key.map(|x| Public(x.public().0.to_vec())),
				local_next_key_public: t.local_next_key.map(|x| Public(x.public().0.to_vec())),
				name: t.name,
			}
		}
	}

	impl From<grandpa::VoterSet<AuthorityId>> for VoterSet {
		fn from(t: grandpa::VoterSet<AuthorityId>) -> VoterSet {
			VoterSet {
				weights: t.weights.iter().map(|(k, v)| (Public(k.0.to_vec()), v.clone().into())).collect(),
				voters: t.voters.iter().map(|(l, r)| (Public(l.0.to_vec()), *r)).collect(),
				threshold: t.threshold,
			}
		}
	}

	impl From<grandpa::VoterInfo> for VoterInfo {
		fn from(t: grandpa::VoterInfo) -> VoterInfo {
			VoterInfo {
				canon_idx: t.canon_idx(),
				weight: t.weight(),
			}
		}
	}

	impl<H, N> From<crfg::VoterSetState<H, N>> for VoterSetState<H, N> {
		fn from(t: crfg::VoterSetState<H, N>) -> VoterSetState<H, N> {
			match t {
				crfg::VoterSetState::Paused(id, round_state) => VoterSetState::Paused(id, round_state.into()),
				crfg::VoterSetState::Live(id, round_state) => VoterSetState::Live(id, round_state.into()),
			}
		}
	}

	impl<H, N> From<grandpa::State<H, N>> for RoundState<H, N> {
		fn from(t: grandpa::State<H, N>) -> RoundState<H, N> {
			RoundState {
				prevote_ghost: t.prevote_ghost,
				finalized: t.finalized,
				estimate: t.estimate,
				completable: t.completable,
			}
		}
	}
}


