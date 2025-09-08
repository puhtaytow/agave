//! This module holds Shred Builder used in production and tests.

// /// Used to determine what Shred content should be generated
// pub enum ShredBuilderContent {
//     /// endless stream of shreds with zeros
//     Empty,
//     /// endless stream of shreds with garbage
//     Random,
//     /// this will make specific number of tick transactions to fill the shreds (so iterator will be finite)
//     Ticks(u64),
//     /// TODO: Entries here instead?
//     Entries(&[Entry]),
//     /// raw data
//     Data(&[u8]),
// }

// /// Used to determine if test should test real scenario or errorgenous one
// pub enum ShredBuilderIndex {
//     Correct(u32),
//     Invalid,
// }

// /// Builder for creating shreds in tests
// pub struct ShredBuilder {
//     keypair: Option<Keypair>,
//     entries: Option<&[Entry]>,
//     slot: Slot,
//     parent_slot: Option<Slot>,
//     chained_merkle_root: Option<Hash>,
//     start_index: Option<ShredBuilderIndex>,
//     reference_tick: Option<u64>,
//     is_last_in_slot: Option<bool>,
// }

// impl ShredBuilder {
//     /// Create new builder instance with default values
//     pub fn new(slot: Slot) -> Self {
//         Self {
//             keypair: None,
//             entries: None,
//             slot,
//             parent_slot: None,
//             chained_merkle_root: None,
//             start_index: None,
//             reference_tick: None,
//             is_last_in_slot: Some(false), // FIXME: maybe should be populated at builder level / None?
//         }
//     }

//     /// Set specific keypair to be used in entries (default is randomly generated)
//     pub fn with_keypair(mut self, keypair: Keypair) -> Self {
//         self.keypair = Some(keypair);
//         self
//     }

//     /// Set specific entries from which shreds would be produced (default is randomly generated)
//     pub fn with_entries(mut self, entries: &[Entry]) -> Self {
//         self.entries = Some(entries);
//         self
//     }

//     /// Set parent slot (default is slot - 1)
//     pub fn with_parent_slot(mut self, parent_slot: Slot) -> Self {
//         self.parent_slot = Some(parent_slot);
//         self
//     }

//     /// Set merkle root hash (default is Hash::default())
//     pub fn with_chained_merkle_root(mut self, hash: Hash) -> Self {
//         self.chained_merkle_root = Some(hash);
//         self
//     }

//     /// Set start index (default is Correct(0u32))
//     /// Passing Invalid variant would result in randomly generated rubbish
//     pub fn with_shred_index(mut self, index: ShredBuilderIndex) -> Self {
//         self.start_index = Some(index);
//         self
//     }

//     /// Set reference tick, used to produce entries (default is / TODO: extend docs)
//     pub fn with_reference_tick(mut self, ticks: u64) -> Self {
//         self.reference_tick = Some(ticks);
//         self
//     }

//     /// Set last in slot (default is false / TODO: extend docs)
//     pub fn last_in_slot(mut self) -> Self {
//         self.is_last_in_slot = Some(true);
//         self
//     }

//     /// Returns an iterator of Shreds
//     pub fn build(self) -> impl Iterator<Item = Shred> {
//         let reference_tick = self.reference_tick.unwrap_or_else(|| 0); // FIXME: dummy
//         let version = 0; // FIXME: does it make sense to parametrize it?

//         let start_index = match self.start_index {
//             Some(variant) => match variant {
//                 ShredBuilderIndex::Correct(index) => {
//                     unimplemented!("generate legit with provided start index")
//                 }
//                 ShredBuilderIndex::Invalid => {
//                     unimplemented!("generate some nonsense configuration")
//                 }
//             },
//             None => 0, // correct with index 0
//         };

//         Shredder::new(
//             self.slot,
//             self.parent_slot.unwrap_or(self.slot.saturating_sub(1)),
//             reference_tick,
//             version,
//         )
//         .expect("should initialize new shredder")
//         .make_merkle_shreds_from_entries(
//             &self.keypair.unwrap_or_else(|| Keypair::new()),
//             self.generate_entries(),
//             self.is_last_in_slot.unwrap_or_default(),
//             chained_merkle_root,
//             start_index, // TODO: when production API change adjust
//             start_index,
//             &ReedSolomonCache::default(),
//             &mut ProcessShredsStats::default(),
//         )
//     }

//     /// Returns single shred for testing
//     pub fn build_single(self) -> Shred {
//         self.build().last().expect("should return single shred")
//     }

//     /// Returns partitioned set of Shreds
//     pub fn build_partitioned(self) -> (Vec<Shred>, Vec<Shred>) {
//         self.build().partition(Shred::is_data)
//     }

//     /// Generates entries outta the builder state
//     fn generate_entries(&self) -> Vec<Entry> {
//         unimplemented!()
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     fn hope_it_wont_be_only_test() {
//         assert_eq!(true, true)
//     }
// }
