// Copyright (C) 2019 Yee Foundation.
//
// This file is part of YeeChain.
//
// YeeChain is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// YeeChain is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with YeeChain.  If not, see <https://www.gnu.org/licenses/>.

//! Network packet message types. These get serialized and put into the lower level protocol payload.

use bitflags::bitflags;
use runtime_primitives::{traits::{Block as BlockT, Header as HeaderT}};
use parity_codec::{Encode, Decode, Input, Output};

/// Type alias for using the message type using block type parameters.
pub type Message<B> = generic::Message<
	<B as BlockT>::Hash,
	<<B as BlockT>::Header as HeaderT>::Number,
	<B as BlockT>::Extrinsic,
>;

/// Type alias for using the status type using block type parameters.
pub type Status<B> = generic::Status<
	<B as BlockT>::Hash,
	<<B as BlockT>::Header as HeaderT>::Number,
>;

/// Generic types.
pub mod generic {
	use parity_codec::{Encode, Decode};
	use network_libp2p::CustomMessage;

	/// A network message.
	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	pub enum Message<Hash, Number, Extrinsic> {
		/// Status packet.
		Status(Status<Hash, Number>),
		/// Extrinsics.
		Extrinsics(Vec<Extrinsic>),
	}

	/// Status sent on connection.
	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	pub struct Status<Hash, Number> {
		/// Protocol version.
		pub version: u32,
		/// Minimum supported version.
		pub min_supported_version: u32,
		/// Best block number.
		pub best_number: Number,
		/// Best block hash.
		pub best_hash: Hash,
		/// Genesis block hash.
		pub genesis_hash: Hash,
		/// Shard num
		pub shard_num: u16,
	}

	#[derive(Debug, Clone)]
	pub enum OutMessage<Extrinsic>{
		/// Extrinsics.
		Extrinsics(Vec<Extrinsic>),
	}

	impl<Hash, Number, Extrinsic> CustomMessage for Message<Hash, Number, Extrinsic>
		where Self: Decode + Encode
	{
		fn into_bytes(self) -> Vec<u8> {
			self.encode()
		}

		fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
			Decode::decode(&mut &bytes[..]).ok_or(())
		}
	}
}
