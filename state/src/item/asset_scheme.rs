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

use std::mem::size_of;

use byteorder::{BigEndian, WriteBytesExt};
use ckey::Address;
use ctypes::ShardId;
use primitives::{H160, H256};
use rlp::{Decodable, DecoderError, Encodable, RlpStream, UntrustedRlp};

use super::asset::Asset;
use crate::CacheableItem;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssetScheme {
    metadata: String,
    amount: u64,
    approver: Option<Address>,
    administrator: Option<Address>,
    allowed_script_hashes: Vec<H160>,
    pool: Vec<Asset>,
}

impl AssetScheme {
    pub fn new(
        metadata: String,
        amount: u64,
        approver: Option<Address>,
        administrator: Option<Address>,
        allowed_script_hashes: Vec<H160>,
    ) -> Self {
        Self {
            metadata,
            amount,
            approver,
            administrator,
            allowed_script_hashes,
            pool: Vec::new(),
        }
    }

    pub fn new_with_pool(
        metadata: String,
        amount: u64,
        approver: Option<Address>,
        administrator: Option<Address>,
        allowed_script_hashes: Vec<H160>,
        pool: Vec<Asset>,
    ) -> Self {
        Self {
            metadata,
            amount,
            approver,
            administrator,
            allowed_script_hashes,
            pool,
        }
    }

    pub fn metadata(&self) -> &String {
        &self.metadata
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }

    pub fn approver(&self) -> &Option<Address> {
        &self.approver
    }

    pub fn administrator(&self) -> &Option<Address> {
        &self.administrator
    }

    pub fn allowed_script_hashes(&self) -> &[H160] {
        &self.allowed_script_hashes
    }

    pub fn is_permissioned(&self) -> bool {
        self.approver.is_some()
    }

    pub fn is_centralized(&self) -> bool {
        self.administrator.is_some()
    }

    pub fn is_allowed_script_hash(&self, lock_script_hash: &H160) -> bool {
        let allowed_hashes = self.allowed_script_hashes();
        allowed_hashes.is_empty() || allowed_hashes.contains(lock_script_hash)
    }

    pub fn init(
        &mut self,
        metadata: String,
        amount: u64,
        approver: Option<Address>,
        administrator: Option<Address>,
        allowed_script_hashes: Vec<H160>,
        pool: Vec<Asset>,
    ) {
        assert_eq!("", &self.metadata);
        assert_eq!(0, self.amount);
        assert_eq!(None, self.approver);
        assert_eq!(None, self.administrator);
        self.metadata = metadata;
        self.amount = amount;
        self.approver = approver;
        self.administrator = administrator;
        self.allowed_script_hashes = allowed_script_hashes;
        self.pool = pool;
    }

    pub fn pool(&self) -> &[Asset] {
        &self.pool
    }

    pub fn change_data(
        &mut self,
        metadata: String,
        approver: Option<Address>,
        administrator: Option<Address>,
        allowed_script_hashes: Vec<H160>,
    ) {
        self.metadata = metadata;
        self.approver = approver;
        self.administrator = administrator;
        self.allowed_script_hashes = allowed_script_hashes;
    }
}

const PREFIX: u8 = super::ASSET_SCHEME_PREFIX;

impl Default for AssetScheme {
    fn default() -> Self {
        Self::new("".to_string(), 0, None, None, Vec::new())
    }
}

impl Encodable for AssetScheme {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(7)
            .append(&PREFIX)
            .append(&self.metadata)
            .append(&self.amount)
            .append(&self.approver)
            .append(&self.administrator)
            .append_list(&self.allowed_script_hashes)
            .append_list(&self.pool);
    }
}

impl Decodable for AssetScheme {
    fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
        if rlp.item_count()? != 7 {
            return Err(DecoderError::RlpInvalidLength)
        }

        let prefix = rlp.val_at::<u8>(0)?;
        if PREFIX != prefix {
            cdebug!(STATE, "{} is not an expected prefix for asset scheme", prefix);
            return Err(DecoderError::Custom("Unexpected prefix"))
        }
        Ok(Self {
            metadata: rlp.val_at(1)?,
            amount: rlp.val_at(2)?,
            approver: rlp.val_at(3)?,
            administrator: rlp.val_at(4)?,
            allowed_script_hashes: rlp.list_at(5)?,
            pool: rlp.list_at(6)?,
        })
    }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetSchemeAddress(H256);

impl_address!(SHARD, AssetSchemeAddress, PREFIX);

impl AssetSchemeAddress {
    pub fn new(tracker: H256, shard_id: ShardId) -> Self {
        let index = ::std::u64::MAX;

        Self::from_transaction_hash_with_shard_id(tracker, index, shard_id)
    }
    pub fn new_with_zero_suffix(shard_id: ShardId) -> Self {
        let mut hash = H256::zero();
        hash[0..2].clone_from_slice(&[PREFIX, 0]);

        let mut shard_id_bytes = Vec::<u8>::new();
        debug_assert_eq!(size_of::<u16>(), size_of::<ShardId>());
        WriteBytesExt::write_u16::<BigEndian>(&mut shard_id_bytes, shard_id).unwrap();
        assert_eq!(2, shard_id_bytes.len());
        hash[2..4].clone_from_slice(&shard_id_bytes);

        AssetSchemeAddress(hash)
    }
}

impl CacheableItem for AssetScheme {
    type Address = AssetSchemeAddress;

    fn is_null(&self) -> bool {
        self.amount == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_from_address() {
        let origin = {
            let mut address;
            'address: loop {
                address = H256::random();
                if address[0] == b'S' {
                    continue
                }
                for a in address.iter().take(6).skip(1) {
                    if *a == 0 {
                        continue 'address
                    }
                }
                break
            }
            address
        };
        let shard_id = 0xBEE;
        let asset_address = AssetSchemeAddress::new(origin, shard_id);
        let hash: H256 = asset_address.into();
        assert_ne!(origin, hash);
        assert_eq!(hash[0..2], [PREFIX, 0]);
        assert_eq!(hash[2..4], [0x0B, 0xEE]); // shard id
    }

    #[test]
    fn shard_id() {
        let origin = H256::random();
        let shard_id = 0xCAA;
        let asset_scheme_address = AssetSchemeAddress::new(origin, shard_id);
        assert_eq!(shard_id, asset_scheme_address.shard_id());
    }

    #[test]
    fn shard_id_from_hash() {
        let hash = {
            let mut hash = H256::random();
            hash[0] = PREFIX;
            hash[1] = 0;
            hash
        };
        assert_eq!(::std::mem::size_of::<u16>(), ::std::mem::size_of::<ShardId>());
        let shard_id = (ShardId::from(hash[2]) << 8) + ShardId::from(hash[3]);
        let asset_scheme_address = AssetSchemeAddress::from_hash(hash).unwrap();
        assert_eq!(shard_id, asset_scheme_address.shard_id());
    }
}
