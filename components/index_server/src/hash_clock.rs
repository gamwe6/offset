use crypto::hash::{sha_512_256, HashResult};
use crypto::crypto_rand::RandValue;

use std::collections::{HashMap, VecDeque};

/// Prefix put before hashing a list of hashes:
const HASH_CLOCK_PREFIX: &[u8] = b"HASH_CLOCK";


/// Information about how a hash value was created.
/// A vector of all hashes (in the correct order), together with an optional corresponding neighbor id.
///
/// Some hashes might not have a corresponding neighbor id. For example: `hash(rand_value)`
/// produced locally.
type HashInfo<N> = Vec<(Option<N>, HashResult)>;

/// A vector of hashes combined with an index into the vector.
pub type HashProof = (Vec<HashResult>, usize);

struct HashClock<N> {
    /// Last hash we received from each neighbor
    neighbor_hashes: HashMap<N, HashResult>,
    /// Maximum length of last_ticks:
    last_ticks_max_len: usize,
    last_ticks: VecDeque<HashResult>,
    last_ticks_map: HashMap<HashResult, HashInfo<N>>,
}

/// Combine a list of hashes into one hash result:
fn hash_hashes(hashes: &[HashResult]) -> HashResult {
    let mut bytes_to_hash = Vec::new();

    // Start with a constant prefix:
    bytes_to_hash.extend_from_slice(&sha_512_256(HASH_CLOCK_PREFIX));

    // Append prefixes:
    for hash in hashes {
        bytes_to_hash.extend_from_slice(hash);
    }

    sha_512_256(&bytes_to_hash)
}

impl<N> HashClock<N> 
where
    N: std::hash::Hash + std::cmp::Eq + Clone,
{
    pub fn new(last_ticks_max_len: usize) -> Self {
        assert!(last_ticks_max_len > 0);

        HashClock {
            neighbor_hashes: HashMap::new(),
            last_ticks_max_len,
            last_ticks: VecDeque::new(),
            last_ticks_map: HashMap::new(),
        }
    }

    /// Insert a new pair of (hash, hash_info)
    fn insert_tick_hash(&mut self, tick_hash: HashResult, hash_info: HashInfo<N>) {
        assert!(self.last_ticks_max_len > 0);
        self.last_ticks.push_back(tick_hash.clone());

        if self.last_ticks.len() > self.last_ticks_max_len {
            let popped_tick_hash = self.last_ticks.pop_front().unwrap();
            self.last_ticks_map.remove(&popped_tick_hash);
        }

        assert!(self.last_ticks.len() <= self.last_ticks_max_len);
        self.last_ticks_map.insert(tick_hash.clone(), hash_info);
    }

    /// Should be called when a new hash is received from a neighbor.
    pub fn update_neighbor_hash(&mut self, neighbor: N, tick_hash: HashResult) -> Option<HashResult> {
        self.neighbor_hashes.insert(neighbor, tick_hash)
    }

    pub fn tick(&mut self, rand_value: RandValue) -> HashResult {
        let mut hash_info = Vec::new();

        let mut hashes = Vec::new();

        let hashed_rand_value = sha_512_256(&rand_value);
        hashes.push(hashed_rand_value.clone());
        hash_info.push((None, hashed_rand_value));

        // Concatenate all hashes, and update hash_info accordingly:
        for (neighbor, hash) in &self.neighbor_hashes {
            hashes.push(hash.clone());
            hash_info.push((Some(neighbor.clone()), hash.clone()));
        }

        let tick_hash = hash_hashes(&hashes);
        self.insert_tick_hash(tick_hash.clone(), hash_info);

        tick_hash
    }

    /// Given a tick hash (that was created in this HashClock), create a HashProof for a neighbor.
    pub fn create_hash_proof(&mut self, tick_hash: HashResult, neighbor: &N) -> Option<HashProof> {
        // Make sure that we have the given tick_hash:
        let hash_info = self.last_ticks_map.get(&tick_hash)?;

        // Find the index of the neighbor at the hashes list:
        let index = hash_info
            .iter()
            .position(|(opt_neighbor, _hash_result)| opt_neighbor.as_ref() == Some(neighbor))?;

        // Prepare a full list of hashes:
        let hashes = hash_info
            .iter()
            .map(|(_opt_neighbor, hash_result)| hash_result)
            .cloned()
            .collect::<Vec<HashResult>>();

        Some((hashes, index))
    }

    /// Verify a chain of hash proof links.
    /// Each link shows that a certain hash is composed from a list of hashes. 
    /// Eventually one of those hashes is a tick_hash created at this HashClock. 
    /// This proves that the `origin_tick_hash` is recent.
    pub fn verify_hash_proof_chain(&self, origin_tick_hash: &HashResult, hash_proof_chain: &[HashProof]) 
        -> Option<HashResult> {

        let mut cur_tick_hash = origin_tick_hash;
        for (hashes, index) in hash_proof_chain {
            if &hash_hashes(hashes) != cur_tick_hash {
                return None;
            }
            cur_tick_hash = hashes.get(*index)?;
        }
        let _ = self.last_ticks_map.get(cur_tick_hash)?;
        Some(cur_tick_hash.clone())
    }
}