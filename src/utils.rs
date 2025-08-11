//! Contains utility functions and traits.

use indexmap::IndexMap;

/// A trait for types/collections that can be empty.
///
/// # Usage
/// O(1) garunteed.
/// Less strict than bounding by [Iterator<Item = T>] or [IntoIterator<Item = T>].
pub trait CollectionExt {
    // 1. Must accurately represent the number of items in the collection.
    // 2. Should only be used for when `len()` is garunteed to be O(1).
    /// Returns the number of items in the collection in O(1) time.
    fn len(&self) -> usize;
    /// Returns `true` if the collection is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl<T> CollectionExt for Vec<T> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<T> CollectionExt for Option<Vec<T>> {
    fn len(&self) -> usize {
        match self {
            Some(v) => v.len(),
            None => 0,
        }
    }
}
impl CollectionExt for String {
    fn len(&self) -> usize {
        self.len()
    }
}
impl CollectionExt for Option<String> {
    fn len(&self) -> usize {
        match self {
            Some(s) => s.len(),
            None => 0,
        }
    }
}
impl<T> CollectionExt for IndexMap<T, T> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<T> CollectionExt for Option<IndexMap<T, T>> {
    fn len(&self) -> usize {
        match self {
            Some(map) => map.len(),
            None => 0,
        }
    }
}