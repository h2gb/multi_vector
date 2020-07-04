//! [![Crate](https://img.shields.io/crates/v/multi_vector.svg)](https://crates.io/crates/multi_vector)
//!
//! An object that holds multiple `BumpyVector`s, and can manage linked entries
//! within a single vector, or between multiple vectors.
//!
//! The purpose of this is to manage pointers and structs in `h2gb`. Sometimes
//! elements across disparate vectors (whether different layers, buffers, etc -
//! doesn't matter) need to be bundled together.
//!
//! This is NOT for references, cross-references, base addresses, or keeping
//! track of logic within a binary. This is the wrong layer for that. I
//! struggled a lot to scope this jussst right, and I started finding that I
//! can't do too much here.
//!
//! # Goal
//!
//! [h2gb](https://github.com/h2gb/libh2gb) is a tool for analyzing binary
//! files. Some binary files will have multiple buffers (like sections in an
//! ELF file, files in a TAR file, etc.). Some of those will have a creator-
//! created relationship with each other, and we want to track that.
//!
//! # Usage
//!
//! Instantiate, add vectors, and add elements to the vectors. All elements
//! added together, as a "group", are linked, and will be removed together.
//!
//! ```
//! use multi_vector::MultiVector;
//!
//! // Create an instance that stores Strings
//! let mut mv: MultiVector<u32> = MultiVector::new();
//!
//! // Create a pair of vectors, one that's 100 elements and one that's 200
//! mv.create_vector("myvector1", 100).unwrap();
//! mv.create_vector("myvector2", 200).unwrap();
//!
//! // Vector names must be unique
//! assert!(mv.create_vector("myvector1", 100).is_err());
//!
//! // It starts with zero entries
//! assert_eq!(0, mv.len());
//!
//! // Populate it with one group
//! mv.insert_entries(vec![
//!     ("myvector1", 111,  0, 10),
//!     ("myvector1", 222, 10, 10),
//! ]);
//!
//! // Now there are two entries
//! assert_eq!(2, mv.len());
//!
//! // Populate with some more values
//! mv.insert_entries(vec![
//!     ("myvector1", 111,  20, 10),
//!     ("myvector2", 222,  0,  10),
//!     ("myvector2", 222,  10, 10),
//! ]);
//!
//! // Now there are five entries!
//! assert_eq!(5, mv.len());
//!
//! // Remove en entry from the first group, note that both entries get
//! // removed
//! assert_eq!(2, mv.remove_entries("myvector1", 15).unwrap().len());
//! assert_eq!(3, mv.len());
//!
//! // myvector1 still has an entry, so we can't remove it
//! assert!(mv.destroy_vector("myvector1").is_err());
//!
//! // Split the final "myvector1" entry out of the group
//! assert!(mv.unlink_entry("myvector1", 20).is_ok());
//!
//! // Remove the final "myvector1" entry.. since we unlinked it, it'll remove
//! // alone
//! assert_eq!(1, mv.remove_entries("myvector1", 20).unwrap().len());
//!
//! // Now there are just two elements left, both in "myvector2"
//! assert_eq!(2, mv.len());
//!
//! // Now we can remove myvector1, since it's empty
//! assert_eq!(100, mv.destroy_vector("myvector1").unwrap());
//! ```
//!
//! # Serialize / deserialize
//!
//! When installed with the 'serialize' feature:
//!
//! ```toml
//! multi_vector = { version = "~0.0.0", features = ["serialize"] }
//! ```
//!
//! Serialization support using [serde](https://serde.rs/) is enabled. The
//! `MultiVector` can be serialized with any of the serializers that Serde
//! supports, such as [ron](https://github.com/ron-rs/ron):
//!
//! ```ignore
//! use multi_vector::MultiVector;
//!
//! // Assumes "serialize" feature is enabled: `multi_vector = { features = ["serialize"] }`
//! let mut mv: MultiVector<String> = MultiVector::new();
//! mv.create_buffer("buf", 100);
//! mv.insert_entries(vec![
//!     ("buf", String::from("a"),  0, 10),
//!     ("buf", String::from("B"), 10, 10),
//! ]);
//!
//! // Serialize
//! let serialized = ron::ser::to_string(&mv).unwrap();
//!
//! // Deserialize
//! let mv: MultiVector<String> = ron::de::from_str(&serialized).unwrap();
//!
//! assert_eq!(2, mv.len());
//! ```

use bumpy_vector::{BumpyVector, BumpyEntry};
use simple_error::{SimpleResult, bail};
use std::collections::HashMap;
use std::fmt::Debug;
use std::mem;

#[cfg(feature = "serialize")]
use serde::{Serialize, Deserialize};

/// Wraps the `T` type in an object with more information.
///
/// This is automatically created by `MultiVector` when inserting elements.
/// It is, however, returned in several places. It helpfully encodes the vector
/// into itself.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
pub struct MultiEntry<T> {
    pub vector: String,
    pub data: T,
    pub linked: Vec<(String, usize)>,
}

/// The primary struct that powers the MultiVector.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
pub struct MultiVector<T>
where
    T: Clone + Debug
{
    // A map of bumpy_vectors, indexed by name
    vectors: HashMap<String, BumpyVector<MultiEntry<T>>>,
}

impl<'a, T> MultiVector<T>
where
    T: Clone + Debug
{

    /// Create a new - empty - instance.
    pub fn new() -> Self {
        MultiVector {
            vectors: HashMap::new(),
        }
    }

    /// Create a vector with a given name and size.
    ///
    /// # Return
    ///
    /// Returns `Ok(())` if the vector is successfully created, or `Err(s)` with
    /// a descriptive error message if it can't be created.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Start with no vectors
    /// assert_eq!(0, mv.vector_count());
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 1000).unwrap();
    ///
    /// // Now there's one vector
    /// assert_eq!(1, mv.vector_count());
    /// ```

    pub fn create_vector(&mut self, name: &str, max_size: usize) -> SimpleResult<()> {
        if self.vectors.contains_key(name) {
            bail!("Vector with that name already exists");
        }

        self.vectors.insert(String::from(name), BumpyVector::new(max_size));

        Ok(())
    }

    /// Remove a vector with the given name.
    ///
    /// Vectors can only be removed if they are empty - otherwise this will
    /// fail. The justification is, we want this to all be compatible with
    /// undo/redo, which means removing items must be replayable. If we do two
    /// things at once (both remove elements and the vector), the API gets
    /// really complicated.
    ///
    /// # Return
    ///
    /// Returns a result containing either the size that the buffer was (for
    /// ease of re-creation in an `undo()` function), or a user-consumeable
    /// error message.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Create a vector of size 1000, then remove it
    /// mv.create_vector("myvector", 1000).unwrap();
    /// assert_eq!(1000, mv.destroy_vector("myvector").unwrap());
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 100).unwrap();
    ///
    /// // Populate it
    /// mv.insert_entry("myvector", 111,  0, 10).unwrap();
    ///
    /// // Fail to remove it
    /// assert!(mv.destroy_vector("myvector").is_err());
    /// ```
    pub fn destroy_vector(&mut self, vector: &str) -> SimpleResult<usize> {
        let v = match self.vectors.get(vector) {
            Some(v) => v,
            None => bail!("Vector with that name does not exist"),
        };

        if v.len() != 0 {
            bail!("Vector is not empty");
        }

        match self.vectors.remove(vector) {
            Some(v) => Ok(v.max_size()),
            None    => bail!("Vector with that name disappeared"),
        }
    }

    /// Remove entries without properly unlinking them.
    ///
    /// This is for internal use only.
    fn _force_remove(&mut self, entries: Vec<(&str, usize)>) {
        for (vector, index) in entries {
            match self.vectors.get_mut(vector) {
                Some(v) => {
                    v.remove(index);
                },
                None => (),
            };
        }
    }

    /// Insert a grouped set of entries into the `MultiVector`.
    ///
    /// The `entries` argument is a vector of tuples, where the first element
    /// is the vector name and the second and onwards effectively describes a
    /// `BumpyEntry` - `(data, index, size)`.
    ///
    /// Entries inserted together are "linked", which means when one is removed,
    /// they are all removed (unless you call `unlink()` on one of them).
    ///
    /// Either all entries are added, or no entries are added; this cannot
    /// leave the vector in a half-way state.
    ///
    /// # Return
    ///
    /// Returns `Ok(())` if the entries were successfully inserted. Returns a
    /// descriptive error otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 1000).unwrap();
    ///
    /// // Verify there are no entries
    /// assert_eq!(0, mv.len());
    ///
    /// // Populate it
    /// mv.insert_entries(vec![
    ///     ("myvector", 111,  0, 10),
    ///     ("myvector", 222, 10, 10),
    /// ]);
    ///
    /// // Now there are two entries
    /// assert_eq!(2, mv.len());
    ///
    /// // Remove one entry
    /// mv.remove_entries("myvector", 5).unwrap();
    ///
    /// // Prove it removes both
    /// assert_eq!(0, mv.len());
    /// ```
    pub fn insert_entries(&mut self, entries: Vec<(&str, T, usize, usize)>) -> SimpleResult<()> {
        // Get the set of references that each entry will store - the vector and
        // location of reach
        let references: Vec<(String, usize)> = entries.iter().map(|(vector, _, index, _)| {
            (String::from(*vector), *index)
        }).collect();

        // We need a way to back out only entries that we've added - handle that
        let mut backtrack: Vec<(&str, usize)> = Vec::new();

        for (vector, data, index, size) in entries {
            // Try and get a handle to the vector
            let v = match self.vectors.get_mut(vector) {
                Some(v) => v,
                None => {
                    // Remove the entries we've added so far + return error
                    self._force_remove(backtrack);
                    bail!("Couldn't find vector: {}", vector);
                }
            };

            // Unwrap the BumpyEntry and make a new one with a MultiEntry instead
            let entry = BumpyEntry {
                entry: MultiEntry {
                    vector: String::from(vector),
                    linked: references.clone(),
                    data: data,
                },
                index: index,
                size: size,
            };

            // Try and insert it into the BumpyVector
            match v.insert(entry) {
                Ok(()) => (),
                Err(e) => {
                    // Remove the entries we've added so far + return error
                    self._force_remove(backtrack);
                    bail!("Error inserting into vector: {}", e);
                }
            }

            // Track what's been added
            backtrack.push((vector, index));
        }

        Ok(())
    }

    /// Insert a single entry, unlinked to others.
    ///
    /// This is a simple wrapper for `insert_entries()`.
    pub fn insert_entry(&mut self, vector: &str, data: T, index: usize, size: usize) -> SimpleResult<()> {
        self.insert_entries(vec![(vector, data, index, size)])
    }

    /// Unlink an entry from its group of entries.
    ///
    /// This will break the connection between an entry and its group.
    ///
    /// # Return
    ///
    /// Returns `Ok(())` on success, or `Err()` with a descriptive error
    /// message on failure.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 1000).unwrap();
    ///
    /// // Verify there are no entries
    /// assert_eq!(0, mv.len());
    ///
    /// // Populate it
    /// mv.insert_entries(vec![
    ///     ("myvector", 111,  0, 10),
    ///     ("myvector", 222, 10, 10),
    /// ]);
    ///
    /// // Now there are two entries
    /// assert_eq!(2, mv.len());
    ///
    /// // Unlink the entries
    /// mv.unlink_entry("myvector", 5);
    ///
    /// // Remove one entry
    /// mv.remove_entries("myvector", 5).unwrap();
    ///
    /// // Prove it only removed one
    /// assert_eq!(1, mv.len());
    /// ```
    pub fn unlink_entry(&mut self, vector: &str, index: usize) -> SimpleResult<()> {
        // This will be a NEW vector of references
        let new_linked: Vec<(String, usize)> = match self.vectors.get_mut(vector) {
            Some(v) => match v.get_mut(index) {
                Some(e) => {
                    // Swap out the linked entry for an empty one
                    let original_links = mem::replace(&mut e.entry.linked, vec![(String::from(vector), e.index)]);

                    // Return the remaining links, with the unlinked one removed
                    original_links.into_iter().filter(|(v, i)| {
                        // Reminder: we can't use `*i == index` here, since
                        // `index` isn't necessarily the start.
                        !(v == vector && *i == e.index)
                    }).collect()
                }
                None => bail!("Couldn't find index {} in vector {}", index, vector),
            },
            None => bail!("Couldn't find vector: {}", vector),
        };

        // Loop through the remaining linked entries and replace the links
        for (vector, index) in new_linked.iter() {
            let v = self.vectors.get_mut(vector).unwrap();
            let e = v.get_mut(*index).unwrap();

            e.entry.linked = new_linked.clone();
        }

        Ok(())
    }

    /// Get a single entry at the requested index.
    ///
    /// # Return
    ///
    /// On success, returns a reference to a `BumpyEntry` that wraps the data
    /// in a `MultiEntry`. I decided to return `MultiEntry` to give easier
    /// access to the `vector` and `references` information.
    ///
    /// If no element exists there, return `None`.
    ///
    /// # Example
    ///
    /// ```
    /// ```
    pub fn get_entry(&self, vector: &str, index: usize) -> Option<&BumpyEntry<MultiEntry<T>>> {
        self.vectors.get(vector)?.get(index)
    }

    /// Get the group of entries, starting at the requested one.
    ///
    /// # Return
    ///
    /// If the entry exists, return the set of entries that were inserted
    /// together, in the same order in which they were inserted.
    ///
    /// Each vector element is returned as `Some(element)`. This is to handle
    /// the unlikely case that a referenced element has disappeared at some
    /// point. That shouldn't be possible, but we need to handle it somehow
    /// (the most obvious place is in deserialization).
    ///
    /// If the original vector or element doesn't exist, return `Err` with
    /// a descriptive error message.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 1000).unwrap();
    ///
    /// // Verify there are no entries
    /// assert_eq!(0, mv.len());
    ///
    /// // Populate it
    /// mv.insert_entries(vec![
    ///     ("myvector", 111,  0, 10),
    ///     ("myvector", 222, 10, 10),
    /// ]);
    ///
    /// // Prove that we get both elements back
    /// assert_eq!(2, mv.get_entries("myvector", 15).unwrap().len());
    ///
    /// // Verify that they are still in the `MultiVector`
    /// assert_eq!(2, mv.len());
    /// ```
    pub fn get_entries(&self, vector: &str, index: usize) -> SimpleResult<Vec<Option<&BumpyEntry<MultiEntry<T>>>>> {
        let linked = match self.vectors.get(vector) {
            Some(v) => match v.get(index) {
                Some(e) => &e.entry.linked,
                None => bail!("Couldn't find index {} in vector {}", index, vector),
            },
            None => bail!("Couldn't find vector: {}", vector),
        };

        let mut results: Vec<Option<&BumpyEntry<MultiEntry<T>>>> = Vec::new();
        for (vector, index) in linked {
            results.push(self.get_entry(vector, *index));
        }

        Ok(results)
    }

    /// Remove and return all entries in a group.
    ///
    /// # Return
    ///
    /// If the entry exists, return the set of entries that were inserted
    /// together, in the same order in which they were inserted.
    ///
    /// Each vector element is returned as `Some(element)`. This is to handle
    /// the unlikely case that a referenced element has disappeared at some
    /// point. That shouldn't be possible, but we need to handle it somehow
    /// (the most obvious place is in deserialization).
    ///
    /// If the original vector or element doesn't exist, return `Err` with
    /// a descriptive error message.
    ///
    /// # Example
    ///
    /// ```
    /// use multi_vector::MultiVector;
    ///
    /// // Create an instance that stores u32 values
    /// let mut mv: MultiVector<u32> = MultiVector::new();
    ///
    /// // Create a vector of size 1000
    /// mv.create_vector("myvector", 1000).unwrap();
    ///
    /// // Verify there are no entries
    /// assert_eq!(0, mv.len());
    ///
    /// // Populate it
    /// mv.insert_entries(vec![
    ///     ("myvector", 111,  0, 10),
    ///     ("myvector", 222, 10, 10),
    /// ]);
    ///
    /// // Prove that we get both elements back
    /// assert_eq!(2, mv.remove_entries("myvector", 15).unwrap().len());
    ///
    /// // Verify that they are gone
    /// assert_eq!(0, mv.len());
    /// ```
    pub fn remove_entries(&mut self, vector: &str, index: usize) -> SimpleResult<Vec<Option<BumpyEntry<MultiEntry<T>>>>> {
        let linked = match self.vectors.get(vector) {
            Some(v) => match v.get(index) {
                Some(e) => e.entry.linked.clone(),
                None => bail!("Couldn't find index {} in vector {}", index, vector),
            },
            None => bail!("Couldn't find vector: {}", vector),
        };


        let mut results: Vec<Option<BumpyEntry<MultiEntry<T>>>> = Vec::new();
        for (vector, index) in linked {
            match self.vectors.get_mut(&vector) {
                Some(v) => {
                    results.push(v.remove(index));
                },
                // Bad reference (shouldn't happen)
                None => results.push(None),
            }
        }

        Ok(results)
    }

    /// Returns the number of vectors in the `MultiVector`.
    pub fn vector_count(&self) -> usize {
        self.vectors.len()
    }

    /// Returns `true` if a vector with the given name exists, false otherwise.
    pub fn vector_exists(&self, vector: &str) -> bool {
        self.vectors.contains_key(vector)
    }

    /// Returns the number of elements in the given vector; `None` otherwise.
    pub fn len_vector(&self, vector: &str) -> Option<usize> {
        let v = self.vectors.get(vector)?;

        Some(v.len())
    }

    /// Returns the max size of the named Vector; `None` if not found.
    pub fn max_size_vector(&self, vector: &str) -> Option<usize> {
        let v = self.vectors.get(vector)?;

        Some(v.max_size())
    }

    /// Returns the total number of entries across all vectors.
    pub fn len(&self) -> usize {
        self.vectors.iter().map(|(_, v)| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_create_and_destroy() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();

        // No vectors to start with
        assert_eq!(0, mv.vector_count());

        // Create a 1000-element vector
        mv.create_vector("name", 1000)?;
        assert_eq!(1, mv.vector_count());

        // Create a second vector
        mv.create_vector("name2", 100)?;
        assert_eq!(2, mv.vector_count());

        // Destroy them
        let removed_size = mv.destroy_vector("name")?;
        assert_eq!(1000, removed_size);
        assert_eq!(1, mv.vector_count());

        let removed_size = mv.destroy_vector("name2")?;
        assert_eq!(100, removed_size);
        assert_eq!(0, mv.vector_count());

        Ok(())
    }

    #[test]
    fn test_destroy_vector_fails_with_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();

        // No vectors to start with
        assert_eq!(0, mv.vector_count());

        // Create a 1000-element vector
        mv.create_vector("name", 1000)?;
        assert_eq!(1, mv.vector_count());

        // Create a second vector
        mv.create_vector("name2", 100)?;
        assert_eq!(2, mv.vector_count());

        // Populate "name2"
        mv.insert_entries(vec![
            ("name2", 123,  10,  10),
            ("name2", 123,  20,  10),
        ])?;

        // "name" is still empty, it can be destroyed
        let removed_size = mv.destroy_vector("name")?;
        assert_eq!(1000, removed_size);
        assert_eq!(1, mv.vector_count());

        // "name2" has an entry, so it can't be removed
        assert!(mv.destroy_vector("name2").is_err());
        assert_eq!(1, mv.vector_count());

        // Remove the entries
        mv.remove_entries("name2", 25)?;

        // Try again
        let removed_size = mv.destroy_vector("name2")?;
        assert_eq!(100, removed_size);
        assert_eq!(0, mv.vector_count());

        Ok(())
    }

    #[test]
    fn test_cant_have_same_vector_twice() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();

        // No vectors to start with
        assert_eq!(0, mv.vector_count());

        // Create a 1000-element vector
        mv.create_vector("name", 1000)?;
        assert_eq!(1, mv.vector_count());

        // Fail to create the same vector again
        assert!(mv.create_vector("name", 100).is_err());
        assert_eq!(1, mv.vector_count());

        // Make sure it's still the original
        assert_eq!(1000, mv.max_size_vector("name").unwrap());

        Ok(())
    }

    #[test]
    fn test_insert_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        let entries: Vec<(&str, u32, usize, usize)> = vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 111,  0,  1),
            ("vector1", 222,  5,  5),
            ("vector1", 333, 10, 10),

            ("vector2", 444, 0, 100),
            ("vector2", 555, 100, 100),
        ];

        // They are empty before
        assert_eq!(0, mv.len_vector("vector1").unwrap());
        assert_eq!(0, mv.len_vector("vector2").unwrap());

        // Insert the entries
        mv.insert_entries(entries)?;

        // They are populated after
        assert_eq!(3, mv.len_vector("vector1").unwrap());
        assert_eq!(2, mv.len_vector("vector2").unwrap());

        let more_entries: Vec<(&str, u32, usize, usize)> = vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 667, 20, 1),
        ];

        // Insert more entries
        mv.insert_entries(more_entries)?;

        // Make sure the vectors are still tracking
        assert_eq!(4, mv.len_vector("vector1").unwrap());

        Ok(())
    }

    #[test]
    fn test_insert_zero_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;

        // Create no entries
        let entries: Vec<(&str, u32, usize, usize)> = vec![];

        // Insert them entries
        mv.insert_entries(entries)?;

        // Ensure nothing was inserted.. I guess?
        assert_eq!(0, mv.len_vector("vector1").unwrap());

        Ok(())
    }

    #[test]
    fn test_insert_invalid_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // Add a couple real entries so we can make sure we don't overwrite
        // or remove them
        mv.insert_entries(vec![
            ("vector1", 123,  0,  10),
            ("vector1", 123, 10,  10),
            ("vector1", 123, 20,  10),
            ("vector2", 123,  0,  10),
        ])?;
        assert_eq!(4, mv.len());

        // Invalid vector
        assert!(mv.insert_entries(vec![
            ("fakevector", 123,  0,  1),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        // Overlapping
        assert!(mv.insert_entries(vec![
            ("vector1", 123,  0,  10),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        // Off the end
        assert!(mv.insert_entries(vec![
            ("vector1", 123,  0,  1000),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        // Zero length
        assert!(mv.insert_entries(vec![
            ("vector1", 123,  0,  0),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        // Overlapping each other
        assert!(mv.insert_entries(vec![
            ("vector1", 123,  10,  10),
            ("vector1", 123,  20,  10),
            ("vector1", 123,  15,   1),
            ("vector1", 123,  50,  10),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        // Multiple entries that overlap - this ensures that we don't
        // accidentally remove things from the vector that we shouldn't
        assert!(mv.insert_entries(vec![
            ("vector1", 123,  0,  10),
            ("vector1", 123, 10,  10),
            ("vector1", 123, 20,  10),
            ("vector2", 123,  0,  10),
        ]).is_err());

        // No entry should be added or removed
        assert_eq!(4, mv.len());

        Ok(())
    }

    #[test]
    fn test_get_entry() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // One group of entries
        mv.insert_entries(vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 111, 0,   1),
            ("vector1", 222, 5,   5),
            ("vector2", 444, 0, 100),
        ])?;

        mv.insert_entries(vec![
            ("vector1", 333, 10, 10),
            ("vector2", 555, 100, 100),
        ])?;

        // Verify that all entries are there
        assert_eq!(5, mv.len());

        // Get a couple entries and make sure they're correct
        assert_eq!(111, mv.get_entry("vector1",   0).unwrap().entry.data);
        assert_eq!(222, mv.get_entry("vector1",   6).unwrap().entry.data);
        assert_eq!(555, mv.get_entry("vector2", 115).unwrap().entry.data);

        // Get some bad entries, make sure they're errors
        assert!(mv.get_entry("badvector", 123).is_none());
        assert!(mv.get_entry("vector1",  1000).is_none());
        assert!(mv.get_entry("vector1",    50).is_none());

        Ok(())
    }

    #[test]
    fn test_get_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // One group of entries
        mv.insert_entries(vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 111, 0,   1),
            ("vector1", 222, 5,   5),
            ("vector2", 444, 0, 100),
        ])?;

        mv.insert_entries(vec![
            ("vector2", 555, 100, 100),
            ("vector1", 333, 10, 10),
        ])?;

        // Verify that all entries are there
        assert_eq!(5, mv.len());

        // Get the first entry at its start
        let group1 = mv.get_entries("vector1", 0)?;
        assert_eq!(3, group1.len());

        assert_eq!(111, group1[0].unwrap().entry.data);
        assert_eq!("vector1", group1[0].unwrap().entry.vector);

        assert_eq!(222, group1[1].unwrap().entry.data);
        assert_eq!("vector1", group1[1].unwrap().entry.vector);

        assert_eq!(444, group1[2].unwrap().entry.data);
        assert_eq!("vector2", group1[2].unwrap().entry.vector);

        // Get the last entry (in the first group) in the middle
        let group1 = mv.get_entries("vector2", 50)?;
        assert_eq!(3, group1.len());

        assert_eq!(111, group1[0].unwrap().entry.data);
        assert_eq!("vector1", group1[0].unwrap().entry.vector);

        assert_eq!(222, group1[1].unwrap().entry.data);
        assert_eq!("vector1", group1[1].unwrap().entry.vector);

        assert_eq!(444, group1[2].unwrap().entry.data);
        assert_eq!("vector2", group1[2].unwrap().entry.vector);

        // Get the second group
        let group2 = mv.get_entries("vector2", 150)?;
        assert_eq!(2, group2.len());

        assert_eq!(555, group2[0].unwrap().entry.data);
        assert_eq!("vector2", group2[0].unwrap().entry.vector);

        assert_eq!(333, group2[1].unwrap().entry.data);
        assert_eq!("vector1", group2[1].unwrap().entry.vector);

        // Get some bad entries, make sure they're errors
        assert!(mv.get_entries("badvector", 123).is_err());
        assert!(mv.get_entries("vector1",  1000).is_err());
        assert!(mv.get_entries("vector1",    50).is_err());

        Ok(())
    }

    #[test]
    fn test_remove_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // One group of entries
        mv.insert_entries(vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 111, 0,   1),
            ("vector1", 222, 5,   5),
            ("vector2", 444, 0, 100),
        ])?;

        mv.insert_entries(vec![
            ("vector2", 555, 100, 100),
            ("vector1", 333,  10,  10),
        ])?;

        // Verify that all entries are there
        assert_eq!(5, mv.len());

        // Get the first entry at its start
        let group1 = mv.remove_entries("vector1", 0)?;

        // The group had 3 entries
        assert_eq!(3, group1.len());

        // Make sure they're actually removed
        assert_eq!(2, mv.len());
        assert!(mv.remove_entries("vector1", 0).is_err());

        assert_eq!(111, group1[0].as_ref().unwrap().entry.data);
        assert_eq!("vector1", group1[0].as_ref().unwrap().entry.vector);

        assert_eq!(222, group1[1].as_ref().unwrap().entry.data);
        assert_eq!("vector1", group1[1].as_ref().unwrap().entry.vector);

        assert_eq!(444, group1[2].as_ref().unwrap().entry.data);
        assert_eq!("vector2", group1[2].as_ref().unwrap().entry.vector);

        // Get the second group
        let group2 = mv.remove_entries("vector2", 150)?;
        assert_eq!(2, group2.len());

        assert_eq!(555, group2[0].as_ref().unwrap().entry.data);
        assert_eq!("vector2", group2[0].as_ref().unwrap().entry.vector);

        assert_eq!(333, group2[1].as_ref().unwrap().entry.data);
        assert_eq!("vector1", group2[1].as_ref().unwrap().entry.vector);

        // Get some bad entries, make sure they're errors
        assert!(mv.remove_entries("badvector", 123).is_err());
        assert!(mv.remove_entries("vector1",  1000).is_err());
        assert!(mv.remove_entries("vector1",    50).is_err());

        Ok(())
    }

    #[test]
    fn test_unlink_entry() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // One group of entries
        mv.insert_entries(vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", 111, 0,   1),
            ("vector1", 222, 5,   5),
            ("vector2", 444, 0, 100), // Will be unlinked for test
        ])?;

        mv.insert_entries(vec![
            ("vector2", 555, 100, 100), // Will be unlinked for test
            ("vector1", 333, 10, 10),
        ])?;

        // Verify that all entries are there
        assert_eq!(5, mv.len());

        // Unlink a couple entries
        mv.unlink_entry("vector2",  50)?;

        mv.unlink_entry("vector2", 150)?;

        // Test error conditions
        assert!(mv.unlink_entry("badvector", 123).is_err());
        assert!(mv.unlink_entry("vector1",  1000).is_err());
        assert!(mv.unlink_entry("vector1",    50).is_err());

        // Remove one
        let removed = mv.remove_entries("vector2", 50)?;
        assert_eq!(1, removed.len());
        assert_eq!(4, mv.len());

        // Remove the other
        let removed = mv.remove_entries("vector2", 100)?;
        assert_eq!(1, removed.len());
        assert_eq!(3, mv.len());

        // Remove the rest of the first group
        let removed = mv.remove_entries("vector1", 0)?;
        assert_eq!(2, removed.len());
        assert_eq!(1, mv.len());

        Ok(())
    }
}
