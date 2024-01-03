// Copyright (C) 2023 Polytope Labs.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! [`IsmpProvider`] implementation

use crate::{
	extrinsic::Extrinsic,
	runtime::api::{
		ismp::Event as Ev,
		runtime_types::{frame_system::EventRecord, hyperbridge_runtime::RuntimeEvent},
	},
	SubstrateClient,
};

use crate::extrinsic::send_unsigned_extrinsic;
use anyhow::anyhow;
use codec::{Decode, Encode};
use debounced::Debounced;
use futures::StreamExt;
use hex_literal::hex;
use ismp::{
	consensus::{ConsensusClientId, ConsensusStateId, StateMachineId},
	events::Event,
	router::Get,
	LeafIndexQuery, SubstrateStateProof,
};
use ismp_rpc::{BlockNumberOrHash, MmrProof};
use primitives::{BoxStream, IsmpHost, IsmpProvider, Query, StateMachineUpdated};
use sp_core::{
	storage::{StorageChangeSet, StorageKey},
	H256,
};
use std::{collections::HashMap, time::Duration};
use subxt::{
	config::{extrinsic_params::BaseExtrinsicParamsBuilder, polkadot::PlainTip, ExtrinsicParams},
	ext::sp_runtime::MultiSignature,
	rpc::Subscription,
	rpc_params,
};

#[async_trait::async_trait]
impl<T, C> IsmpProvider for SubstrateClient<T, C>
where
	C: subxt::Config + Send + Sync + Clone,
	C::Header: Send + Sync,
	<C::ExtrinsicParams as ExtrinsicParams<C::Hash>>::OtherParams:
		Default + Send + Sync + From<BaseExtrinsicParamsBuilder<C, PlainTip>>,
	C::AccountId:
		From<sp_core::crypto::AccountId32> + Into<C::Address> + Clone + 'static + Send + Sync,
	C::Signature: From<MultiSignature> + Send + Sync,
	T: IsmpHost + Send + Sync,
{
	async fn query_consensus_state(
		&self,
		at: Option<u64>,
		id: ConsensusClientId,
	) -> Result<Vec<u8>, anyhow::Error> {
		let params = rpc_params![at, id];
		let response = self.client.rpc().request("ismp_queryConsensusState", params).await?;

		Ok(response)
	}

	async fn query_latest_height(&self, id: StateMachineId) -> Result<u32, anyhow::Error> {
		let params = rpc_params![id];
		let response =
			self.client.rpc().request("ismp_queryStateMachineLatestHeight", params).await?;

		Ok(response)
	}

	async fn query_latest_messaging_height(
		&self,
		id: StateMachineId,
	) -> Result<u64, anyhow::Error> {
		let params = rpc_params![id];
		let response = self.client.rpc().request("ismp_queryLatestMessagingHeight", params).await?;

		Ok(response)
	}

	async fn query_consensus_update_time(
		&self,
		id: ConsensusClientId,
	) -> Result<Duration, anyhow::Error> {
		let params = rpc_params![id];
		let response: u64 =
			self.client.rpc().request("ismp_queryConsensusUpdateTime", params).await?;

		Ok(Duration::from_secs(response))
	}

	async fn query_requests_proof(
		&self,
		at: u64,
		keys: Vec<Query>,
	) -> Result<Vec<u8>, anyhow::Error> {
		let params = rpc_params![at, convert_queries(keys)];
		let response: ismp_rpc::Proof =
			self.client.rpc().request("ismp_queryRequestsMmrProof", params).await?;
		let proof: MmrProof<H256> = Decode::decode(&mut &*response.proof)?;
		Ok(proof.encode())
	}

	async fn query_responses_proof(
		&self,
		at: u64,
		keys: Vec<Query>,
	) -> Result<Vec<u8>, anyhow::Error> {
		let params = rpc_params![at, convert_queries(keys)];
		let response: ismp_rpc::Proof =
			self.client.rpc().request("ismp_queryResponsesMmrProof", params).await?;
		let proof: MmrProof<H256> = Decode::decode(&mut &*response.proof)?;
		Ok(proof.encode())
	}

	async fn query_state_proof(
		&self,
		at: u64,
		keys: Vec<Vec<u8>>,
	) -> Result<Vec<u8>, anyhow::Error> {
		let params = rpc_params![at, keys];
		let response: ismp_rpc::Proof =
			self.client.rpc().request("ismp_queryStateProof", params).await?;

		let storage_proof: Vec<Vec<u8>> = Decode::decode(&mut &*response.proof)?;
		let proof = SubstrateStateProof { hasher: self.hashing.clone(), storage_proof };
		Ok(proof.encode())
	}

	async fn query_ismp_events(
		&self,
		previous_height: u64,
		event: StateMachineUpdated,
	) -> Result<Vec<Event>, anyhow::Error> {
		let range = (previous_height + 1)..=event.latest_height;
		if range.is_empty() {
			return Ok(Default::default())
		}
		let block_numbers: Vec<BlockNumberOrHash<sp_core::H256>> = range
			.clone()
			.into_iter()
			.map(|block_height| BlockNumberOrHash::Number(block_height as u32))
			.collect();
		log::info!("querying: {range:?}");

		let params = rpc_params![block_numbers];
		let response: HashMap<String, Vec<Event>> =
			self.client.rpc().request("ismp_queryEvents", params).await?;
		let events = response.values().into_iter().cloned().flatten().collect();
		Ok(events)
	}

	async fn query_pending_get_requests(&self, height: u64) -> Result<Vec<Get>, anyhow::Error> {
		let response = self
			.client
			.rpc()
			.request("ismp_pendingGetRequests", rpc_params![height])
			.await?;
		Ok(response)
	}

	fn name(&self) -> String {
		self.state_machine.to_string()
	}

	fn state_machine_id(&self) -> StateMachineId {
		StateMachineId { state_id: self.state_machine, consensus_state_id: self.consensus_state_id }
	}

	fn block_max_gas(&self) -> u64 {
		todo!()
	}

	fn initial_height(&self) -> u64 {
		self.initial_height
	}

	async fn estimate_gas(
		&self,
		_msg: Vec<ismp::messaging::Message>,
	) -> Result<u64, anyhow::Error> {
		todo!()
	}

	async fn state_machine_update_notification(
		&self,
		counterparty_state_id: StateMachineId,
	) -> Result<BoxStream<StateMachineUpdated>, anyhow::Error> {
		let keys = vec![system_events_key()];
		let subscription = self
			.client
			.rpc()
			.subscribe::<StorageChangeSet<H256>>(
				"state_subscribeStorage",
				rpc_params![keys],
				"state_unsubscribeStorage",
			)
			.await
			.expect("Storage subscription failed");

		Ok(filter_map_system_events(subscription, counterparty_state_id))
	}

	async fn submit(&self, messages: Vec<ismp::messaging::Message>) -> Result<(), anyhow::Error> {
		let mut futs = vec![];
		for msg in messages {
			let call = vec![msg].encode();
			let extrinsic = Extrinsic::new("Ismp", "handle", call);
			futs.push(send_unsigned_extrinsic(&self.client, extrinsic))
		}
		let _ = futures::future::join_all(futs).await;
		Ok(())
	}

	async fn query_challenge_period(
		&self,
		id: ConsensusStateId,
	) -> Result<Duration, anyhow::Error> {
		let params = rpc_params![id];
		let response: u64 = self.client.rpc().request("ismp_queryChallengePeriod", params).await?;

		Ok(Duration::from_secs(response))
	}

	async fn query_timestamp(&self) -> Result<Duration, anyhow::Error> {
		let timestamp_key =
			hex!("f0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb").to_vec();
		let response = self
			.client
			.rpc()
			.storage(&timestamp_key, None)
			.await?
			.ok_or_else(|| anyhow!("Failed to fetch timestamp"))?;
		let timestamp: u64 = codec::Decode::decode(&mut response.0.as_slice())?;

		Ok(Duration::from_millis(timestamp))
	}
}

fn convert_queries(queries: Vec<Query>) -> Vec<LeafIndexQuery> {
	queries
		.into_iter()
		.map(|query| LeafIndexQuery {
			source_chain: query.source_chain,
			dest_chain: query.dest_chain,
			nonce: query.nonce,
		})
		.collect()
}

// The storage key needed to access events.
pub fn system_events_key() -> StorageKey {
	let mut storage_key = sp_core::twox_128(b"System").to_vec();
	storage_key.extend(sp_core::twox_128(b"Events").to_vec());
	StorageKey(storage_key)
}

pub fn filter_map_system_events(
	subscription: Subscription<StorageChangeSet<H256>>,
	counterparty_state_id: StateMachineId,
) -> BoxStream<StateMachineUpdated> {
	let debounced_sub = Debounced::new(subscription, Duration::from_secs(4));
	let stream = debounced_sub.filter_map(move |change_set| {
		if let Ok(change_set) = change_set {
			let records = change_set
				.changes
				.into_iter()
				.filter_map(|(_, change)| {
					change.and_then(|data| {
						<Vec<EventRecord<RuntimeEvent, H256>> as codec::Decode>::decode(
							&mut data.0.as_slice(),
						)
						.ok()
						.map(|records| {
							records
								.into_iter()
								.filter_map(|record| match record.event {
									RuntimeEvent::Ismp(Ev::StateMachineUpdated {
										state_machine_id,
										latest_height,
									}) => {
										if counterparty_state_id.encode() ==
											state_machine_id.encode()
										{
											Some(StateMachineUpdated {
												state_machine_id: counterparty_state_id,
												latest_height,
											})
										} else {
											None
										}
									},
									_ => None,
								})
								.collect::<Vec<_>>()
						})
					})
				})
				.flatten()
				.collect::<Vec<_>>();
			return futures::future::ready(records.last().cloned().map(|ev| Ok(ev)))
		}

		futures::future::ready(None)
	});

	Box::pin(stream)
}
