// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#![warn(unused_extern_crates)]
#![warn(missing_docs)]

//! Substrate-specific P2P networking: synchronizing blocks, propagating BFT messages.
//! Allows attachment of an optional subprotocol for chain-specific requests.
//! 
//! **Important**: This crate is unstable and the API and usage may change.
//!

mod service;
//mod sync;
#[macro_use]
//mod protocol;
//mod chain;
//mod blocks;
//mod on_demand;
//mod util;
//pub mod config;
//pub mod consensus_gossip;
//pub mod error;
//pub mod message;
//pub mod specialization;

pub use chain::Client as ClientHandle;
pub use service::{Service, FetchFuture, ImportQueueMsg, ImportQueuePort};
pub use protocol::{ProtocolStatus, PeerInfo, Context, FromNetworkMsg};
pub use sync::{Status as SyncStatus, SyncState};
pub use substrate_network::{
	identity, multiaddr,
	ProtocolId, Severity, Multiaddr,
	NetworkState, NetworkStatePeer, NetworkStateNotConnectedPeer, NetworkStatePeerEndpoint,
	build_multiaddr, PeerId, PublicKey, IdentifySpecialization, DefaultIdentifySpecialization, IdentifyInfo,
};
pub use message::{generic as generic_message, RequestId, Status as StatusMessage};
pub use error::Error;
pub use on_demand::{OnDemand, OnDemandService, RemoteResponse};
#[doc(hidden)]
pub use runtime_primitives::traits::Block as BlockT;

pub use substrate_network::sync;
pub use substrate_network::protocol;
pub use substrate_network::chain;
pub use substrate_network::blocks;
pub use substrate_network::on_demand;
pub use substrate_network::util;
pub use substrate_network::config;
pub use substrate_network::consensus_gossip;
pub use substrate_network::error;
pub use substrate_network::message;
pub use substrate_network::specialization;
