use std::collections::HashMap;
use bumpy_vector::{BumpyVector, BumpyEntry};
use std::fmt::Debug;
use simple_error::{SimpleResult, bail};

#[cfg(feature = "serialize")]
use serde::{Serialize, Deserialize};

/*

This is a whole new idea..

This class is a way to 'group' entries that are inserted together. The design
is entirely for h2gb's arrays, structs, and so on.

In other words, it's for storing non-contiguous stuff that could span multiple
vectors, grouped together so you have to remove them all at once. I don't think
we need a way to "break" entries, but we can worry about that later.

When creating an entry, multiple entries over multiple vectors can be created.
When you remove one, you remove all of them.

This is NOT for references, or cross references, or loops, or anything like
that.

*/

// Basically a BumpyEntry + a string for the name
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
pub struct MultiEntry<T> {
    pub vector: String,
    pub data: T,
    pub linked: Vec<(String, usize)>,
}

impl<T> MultiEntry<T> {
    fn wrap_entry(vector: &str, entry: BumpyEntry<T>, linked: Vec<(String, usize)>) -> BumpyEntry<MultiEntry<T>> {
        BumpyEntry {
            entry: MultiEntry {
                vector: String::from(vector),
                linked: linked,
                data: entry.entry,
            },
            index: entry.index,
            size: entry.size,
        }
    }

    // fn unwrap_entry(entry: BumpyEntry<MultiEntry<T>>) -> (String, BumpyEntry<T>, Vec<(String, usize)>) {
    //     let vector = entry.entry.vector;
    //     let data = entry.entry.data;
    //     let linked = entry.entry.linked;

    //     (vector, BumpyEntry { entry: data, index: entry.index, size: entry.size }, linked)
    // }
}

// impl<T> From<(String, Vec<(String, usize)>, BumpyEntry<T>)> for MultiEntry<T> {
//     fn from(o: (String, BumpyEntry<T>)) -> Self {
//         MultiEntry {
//           vector: o.0,
//           entry: o.1,
//         }
//     }
// }

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

    pub fn new() -> Self {
        MultiVector {
            vectors: HashMap::new(),
        }
    }

    pub fn create_vector(&mut self, name: &str, max_size: usize) -> SimpleResult<()> {
        if self.vectors.contains_key(name) {
            bail!("Vector with that name already exists");
        }

        self.vectors.insert(String::from(name), BumpyVector::new(max_size));

        Ok(())
    }

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

    pub fn insert_entries(&mut self, entries: Vec<(&str, BumpyEntry<T>)>) -> SimpleResult<()> {
        // Clone the full set so we can backtrack if things go wrong
        // XXX: This is JUST for testing! This is incredibly slow!
        let backtrack = self.vectors.clone();

        // Get the set of references that each entry will store - the vector and
        // location of reach
        let references: Vec<(String, usize)> = entries.iter().map(|(vector, entry)| {
            (String::from(*vector), entry.index)
        }).collect();

        for (vector, entry) in entries {
            // Try and get a handle to the vector
            let v = match self.vectors.get_mut(vector) {
                Some(v) => v,
                None => {
                    // Remove the entries we've added so far + return error
                    self.vectors = backtrack;
                    bail!("Couldn't find vector: {}", vector);
                }
            };

            // Unwrap the BumpyEntry so we can make a new one with a MultiEntry
            let entry = MultiEntry::wrap_entry(vector, entry, references.clone());

            // Try and insert it into the BumpyVector
            match v.insert(entry) {
                Ok(()) => (),
                Err(e) => {
                    // Remove the entries we've added so far + return error
                    self.vectors = backtrack;
                    bail!("Error inserting into vector: {}", e);
                }
            }
        }

        Ok(())
    }

    // // Remove from a group
    // pub fn unlink_entry(_vector: &str, _address: usize) {
    // }

    pub fn get_entry(&self, vector: &str, address: usize) -> Option<&BumpyEntry<MultiEntry<T>>> {
        let v = self.vectors.get(vector)?;

        v.get(address)
    }

    // This guarantees that the response vector will have entries in the same
    // order as they were inserted. In case that matters.
    pub fn get_entries(&self, vector: &str, address: usize) -> SimpleResult<Vec<Option<&BumpyEntry<MultiEntry<T>>>>> {
        let linked = match self.vectors.get(vector) {
            Some(v) => match v.get(address) {
                Some(e) => &e.entry.linked,
                None => bail!("Couldn't find address {} in vector {}", address, vector),
            },
            None => bail!("Couldn't find vector: {}", vector),
        };

        let mut results: Vec<Option<&BumpyEntry<MultiEntry<T>>>> = Vec::new();
        for (vector, address) in linked {
            results.push(self.get_entry(vector, *address));
        }

        Ok(results)
    }

    pub fn remove_entries(&mut self, vector: &str, address: usize) -> SimpleResult<Vec<Option<BumpyEntry<MultiEntry<T>>>>> {
        let linked = match self.vectors.get(vector) {
            Some(v) => match v.get(address) {
                Some(e) => e.entry.linked.clone(),
                None => bail!("Couldn't find address {} in vector {}", address, vector),
            },
            None => bail!("Couldn't find vector: {}", vector),
        };


        let mut results: Vec<Option<BumpyEntry<MultiEntry<T>>>> = Vec::new();
        for (vector, address) in linked {
            match self.vectors.get_mut(&vector) {
                Some(v) => {
                    results.push(v.remove(address));
                },
                // Bad reference (shouldn't happen)
                None => results.push(None),
            }
        }

        Ok(results)
    }

    // Get the number of vectors
    pub fn vector_count(&self) -> usize {
        self.vectors.len()
    }

    // Is the vector a member of the MultiVector?
    pub fn vector_exists(&self, vector: &str) -> bool {
        self.vectors.contains_key(vector)
    }

    // Get the length of a vector, if it exists
    pub fn len_vector(&self, vector: &str) -> Option<usize> {
        let v = self.vectors.get(vector)?;

        Some(v.len())
    }

    // Get the length of a vector, if it exists
    pub fn max_size_vector(&self, vector: &str) -> Option<usize> {
        let v = self.vectors.get(vector)?;

        Some(v.max_size())
    }

    // Get the length of ALL vectors
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
            ("name2", (123,  10,  10).into()),
            ("name2", (123,  20,  10).into()),
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

        let entries: Vec<(&str, BumpyEntry<u32>)> = vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", (111,  0,  1).into()),
            ("vector1", (222,  5,  5).into()),
            ("vector1", (333, 10, 10).into()),

            ("vector2", (444, 0, 100).into()),
            ("vector2", (555, 100, 100).into()),
        ];

        // They are empty before
        assert_eq!(0, mv.len_vector("vector1").unwrap());
        assert_eq!(0, mv.len_vector("vector2").unwrap());

        // Insert the entries
        mv.insert_entries(entries)?;

        // They are populated after
        assert_eq!(3, mv.len_vector("vector1").unwrap());
        assert_eq!(2, mv.len_vector("vector2").unwrap());

        let more_entries: Vec<(&str, BumpyEntry<u32>)> = vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", (666, 20, 1).into()),
        ];

        // Insert more entries
        mv.insert_entries(more_entries)?;

        // Make sure the vectors are still tracking
        assert_eq!(4, mv.len_vector("vector1").unwrap());

        Ok(())
    }

    #[test]
    fn test_insert_invalid_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // Invalid vector
        assert!(mv.insert_entries(vec![
            ("fakevector", (123,  0,  1).into()),
        ]).is_err());

        // No entry should be added
        assert_eq!(0, mv.len());

        // Off the end
        assert!(mv.insert_entries(vec![
            ("vector1", (123,  0,  1000).into()),
        ]).is_err());

        // No entry should be added
        assert_eq!(0, mv.len());

        // Zero length
        assert!(mv.insert_entries(vec![
            ("vector1", (123,  0,  0).into()),
        ]).is_err());

        // No entry should be added
        assert_eq!(0, mv.len());

        // Overlapping entries
        assert!(mv.insert_entries(vec![
            ("vector1", (123,  10,  10).into()),
            ("vector1", (123,  20,  10).into()),
            ("vector1", (123,  15,   1).into()),
            ("vector1", (123,  50,  10).into()),
        ]).is_err());

        // No entry should be added - this is the most important one, since the
        // entries above need to be backed out
        assert_eq!(0, mv.len());

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
            ("vector1", (111, 0,   1).into()),
            ("vector1", (222, 5,   5).into()),
            ("vector2", (444, 0, 100).into()),
        ])?;

        mv.insert_entries(vec![
            ("vector1", (333, 10, 10).into()),
            ("vector2", (555, 100, 100).into()),
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
            ("vector1", (111, 0,   1).into()),
            ("vector1", (222, 5,   5).into()),
            ("vector2", (444, 0, 100).into()),
        ])?;

        mv.insert_entries(vec![
            ("vector2", (555, 100, 100).into()),
            ("vector1", (333, 10, 10).into()),
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
    fn test_unlink_entry() {
    }

    #[test]
    fn test_remove_entries() -> SimpleResult<()> {
        let mut mv: MultiVector<u32> = MultiVector::new();
        mv.create_vector("vector1", 100)?;
        mv.create_vector("vector2", 200)?;

        // One group of entries
        mv.insert_entries(vec![
            // (vector_name, ( data, index, length ) )
            ("vector1", (111, 0,   1).into()),
            ("vector1", (222, 5,   5).into()),
            ("vector2", (444, 0, 100).into()),
        ])?;

        mv.insert_entries(vec![
            ("vector2", (555, 100, 100).into()),
            ("vector1", (333, 10, 10).into()),
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
}
