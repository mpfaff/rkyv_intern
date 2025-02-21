use alloc::borrow::ToOwned;
use core::{borrow::Borrow, error::Error, fmt, hash::{BuildHasher, Hash}, num::NonZeroUsize};

use hashbrown::HashMap;
use rkyv::rancor::{fail, Source};

use crate::{Interning, InterningState};

/// An entry in the interner.
pub struct Entry {
    pos: Option<NonZeroUsize>,
    /// The number of references to the value.
    #[cfg(feature = "statistics")]
    pub ref_cnt: NonZeroUsize,
}

/// A general-purpose value interner.
pub struct Interner<T> {
    value_to_pos: HashMap<T, Entry>,
}

impl<T> Interner<T> {
    /// Returns a new, empty interner.
    pub fn new() -> Self {
        Self {
            value_to_pos: HashMap::new(),
        }
    }

    /// The number of interned values.
    pub fn len(&self) -> usize {
        self.value_to_pos.len()
    }

    /// The interned values.
    pub fn iter(&self) -> hashbrown::hash_map::Iter<'_, T, Entry> {
        self.value_to_pos.iter()
    }
}

impl<T> Default for Interner<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct NotStarted;

impl fmt::Display for NotStarted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "value was not started interning")
    }
}

impl Error for NotStarted {}

#[derive(Debug)]
struct AlreadyFinished;

impl fmt::Display for AlreadyFinished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "value was already finished interning")
    }
}

impl Error for AlreadyFinished {}

impl<T, E> Interning<T, E> for Interner<T::Owned>
where
    T::Owned: Hash + Eq + Borrow<T>,
    T: Hash + Eq + ToOwned + ?Sized,
    E: Source,
{
    type State<'a> = (&'a T, u64) where T: 'a;

    fn start_interning<'a>(&mut self, value: &'a T) -> InterningState<Self::State<'a>> {
        use hashbrown::hash_map::RawEntryMut::*;
        let hash = self.value_to_pos.hasher().hash_one(value);
        match self.value_to_pos.raw_entry_mut().from_key_hashed_nocheck(hash, value) {
            Occupied(mut entry) => {
                let entry = entry.get_mut();
                #[cfg(feature = "statistics")]
                {
                    entry.ref_cnt = entry.ref_cnt.checked_add(1).unwrap();
                }
                match entry.pos {
                    None => InterningState::Pending,
                    Some(pos) => InterningState::Finished(pos.get() - 1),
                }
            },
            Vacant(entry) => {
                entry.insert(value.to_owned(), Entry {
                    pos: None,
                    #[cfg(feature = "statistics")]
                    ref_cnt: NonZeroUsize::new(1).unwrap(),
                });
                InterningState::Started((value, hash))
            }
        }
    }

    fn finish_interning(&mut self, state: Self::State<'_>, pos: usize) -> Result<(), E> {
        use hashbrown::hash_map::RawEntryMut::*;
        let (value, hash) = state;
        match self.value_to_pos.raw_entry_mut().from_key_hashed_nocheck(hash, value) {
            Occupied(entry) => match &mut entry.into_mut().pos {
                Some(_) => fail!(AlreadyFinished),
                x => {
                    *x = Some(NonZeroUsize::new(pos + 1).unwrap());
                    Ok(())
                }
            }
            Vacant(_) => fail!(NotStarted),
        }
    }
}
