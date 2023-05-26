
use move_core_types::language_storage::TypeTag;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn coin_pair_hash(coin_a: &TypeTag, coin_b: &TypeTag) -> u64 {
    typetag_set_hash(&[coin_a, coin_b])
}

// We want there to be collisions for sets
pub fn typetag_set_hash(typetags: &[&TypeTag]) -> u64 {
    typetags
        .iter()
        .fold(0, |composite, typetag| {
            composite ^ calculate_hash(typetag)
        })
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}