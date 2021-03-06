// Copyright 2018 Kodebox, Inc.
// This file is part of CodeChain.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::collections::HashSet;
use std::sync::Arc;

use cnetwork::{Api, DiscoveryApi, IntoSocketAddr, NetworkExtension, NodeId, RoutingTable};
use ctimer::{TimeoutHandler, TimerToken};
use parking_lot::RwLock;
use rand::prelude::SliceRandom;
use rand::thread_rng;
use rlp::{Decodable, Encodable, UntrustedRlp};
use time::Duration;

use super::Config;
use super::Message;

pub struct Extension {
    config: Config,
    routing_table: RwLock<Option<Arc<RoutingTable>>>,
    api: RwLock<Option<Arc<Api>>>,
    nodes: RwLock<HashSet<NodeId>>,
}

impl Extension {
    #![cfg_attr(feature = "cargo-clippy", allow(clippy::new_ret_no_self))]
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            routing_table: RwLock::new(None),
            api: RwLock::new(None),
            nodes: RwLock::new(HashSet::new()),
        })
    }
}

const REFRESH_TOKEN: TimerToken = 0;

impl NetworkExtension for Extension {
    fn name(&self) -> &'static str {
        "unstructured-discovery"
    }

    fn need_encryption(&self) -> bool {
        false
    }

    fn versions(&self) -> &[u64] {
        const VERSIONS: &[u64] = &[0];
        &VERSIONS
    }

    fn on_initialize(&self, api: Arc<Api>) {
        let mut api_lock = self.api.write();

        api.set_timer(REFRESH_TOKEN, Duration::milliseconds(i64::from(self.config.t_refresh)))
            .expect("Refresh msut be registered");

        *api_lock = Some(api);
    }

    fn on_node_added(&self, node: &NodeId, _version: u64) {
        let api = self.api.read();
        let mut nodes = self.nodes.write();
        nodes.insert(*node);
        if let Some(api) = api.as_ref() {
            api.send(&node, &Message::Request(self.config.bucket_size).rlp_bytes());
        }
    }

    fn on_node_removed(&self, node: &NodeId) {
        let mut nodes = self.nodes.write();
        nodes.remove(node);
    }

    fn on_message(&self, node: &NodeId, message: &[u8]) {
        let message = match Message::decode(&UntrustedRlp::new(&message)) {
            Ok(message) => message,
            Err(err) => {
                cwarn!(DISCOVERY, "Invalid message from {} : {:?}", node, err);
                return
            }
        };
        match message {
            Message::Request(len) => {
                let routing_table = self.routing_table.read();
                let api = self.api.read();
                if let (Some(api), Some(routing_table)) = (&*api, &*routing_table) {
                    let mut addresses =
                        routing_table.reachable_addresses(&node.into_addr()).into_iter().collect::<Vec<_>>();
                    addresses.shuffle(&mut thread_rng());
                    let addresses =
                        addresses.into_iter().take(::std::cmp::min(self.config.bucket_size, len) as usize).collect();
                    let response = Message::Response(addresses).rlp_bytes();
                    api.send(&node, &response);
                }
            }
            Message::Response(addresses) => {
                let routing_table = self.routing_table.read();
                match routing_table.as_ref() {
                    None => cwarn!(DISCOVERY, "No routing table"),
                    Some(routing_table) => {
                        for address in addresses.into_iter() {
                            routing_table.add_candidate(address);
                        }
                    }
                }
            }
        }
    }
}

impl TimeoutHandler for Extension {
    fn on_timeout(&self, timer: TimerToken) {
        match timer {
            REFRESH_TOKEN => {
                let mut api = self.api.read();
                let nodes = self.nodes.read();

                if let Some(api) = api.as_ref() {
                    let request = Message::Request(self.config.bucket_size).rlp_bytes();
                    for node in nodes.iter() {
                        api.send(&node, &request);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DiscoveryApi for Extension {
    fn set_routing_table(&self, routing_table: Arc<RoutingTable>) {
        *self.routing_table.write() = Some(routing_table);
    }
}
