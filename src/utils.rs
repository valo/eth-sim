use ethers_core::types::{Transaction, H256};
use revm::primitives::TransactTo;

#[inline]
pub fn u256_to_ru256(u: ethers_core::types::U256) -> revm::primitives::U256 {
    let mut buffer = [0u8; 32];
    u.to_little_endian(buffer.as_mut_slice());
    revm::primitives::U256::from_le_bytes(buffer)
}

#[inline]
pub fn h256_to_u256_be(storage: H256) -> ethers_core::types::U256 {
    ethers_core::types::U256::from_big_endian(storage.as_bytes())
}

#[inline]
pub fn h160_to_b160(h: ethers_core::types::H160) -> revm::primitives::B160 {
    revm::primitives::B160(h.0)
}

#[inline]
pub fn h256_to_b256(h: ethers_core::types::H256) -> revm::primitives::B256 {
    revm::primitives::B256(h.0)
}

pub fn configure_tx_env(env: &mut revm::primitives::Env, tx: &Transaction) {
    env.tx.caller = h160_to_b160(tx.from);
    env.tx.gas_limit = tx.gas.as_u64();
    env.tx.gas_price = tx.gas_price.unwrap_or_default().into();
    env.tx.gas_priority_fee = tx.max_priority_fee_per_gas.map(Into::into);
    env.tx.nonce = Some(tx.nonce.as_u64());
    env.tx.access_list = tx
        .access_list
        .clone()
        .unwrap_or_default()
        .0
        .into_iter()
        .map(|item| {
            (
                h160_to_b160(item.address),
                item.storage_keys
                    .into_iter()
                    .map(h256_to_u256_be)
                    .map(u256_to_ru256)
                    .collect(),
            )
        })
        .collect();
    env.tx.value = tx.value.into();
    env.tx.data = tx.input.0.clone();
    env.tx.transact_to = tx
        .to
        .map(h160_to_b160)
        .map(TransactTo::Call)
        .unwrap_or_else(TransactTo::create)
}
