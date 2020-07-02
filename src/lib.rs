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
struct MultiEntry<T> {
    vector: String,
    data: T,
    friends: Vec<(String, usize)>,
}

impl<T> MultiEntry<T> {
    fn wrap_entry(vector: &str, entry: BumpyEntry<T>, friends: Vec<(String, usize)>) -> BumpyEntry<MultiEntry<T>> {
        BumpyEntry {
            entry: MultiEntry {
                vector: String::from(vector),
                friends: friends,
                data: entry.entry,
            },
            index: entry.index,
            size: entry.size,
        }
    }

    // fn unwrap_entry(entry: BumpyEntry<MultiEntry<T>>) -> (String, BumpyEntry<T>, Vec<(String, usize)>) {
    //     let vector = entry.entry.vector;
    //     let data = entry.entry.data;
    //     let friends = entry.entry.friends;

    //     (vector, BumpyEntry { entry: data, index: entry.index, size: entry.size }, friends)
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

        println!("==");
        println!("References: {:?}", references);
        println!("==");

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

    // fn _remove_entry(&mut self, vector: &str, address: usize) -> Option<BumpyEntry<MultiEntry<T>>> {
    //     self.vectors.get_mut(vector)?.remove(address)
    // }

    // fn _get_entry(&self, vector: &str, address: usize) -> Option<&BumpyEntry<MultiEntry<T>>> {
    //     self.vectors.get(vector)?.get(address)
    // }

    // pub fn remove_entries(&mut self, vector: &str, address: usize) -> Result<Vec<BumpyEntry<T>>, &'static str> {
    //     Ok(vec![])
    // }

    // // Remove from a group
    // pub fn unlink_entry(_vector: &str, _address: usize) {
    // }

    // pub fn get_entry(_vector: String, _address: usize) -> Option<MultiEntry<&'a T>> {
    //     None
    // }

    // pub fn get_entries(_vector: String, _address: usize) -> Option<Vec<MultiEntry<&'a T>>> {
    //     None
    // }

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
    fn test_insert_invalid_entries() {
    }

    #[test]
    fn test_remove_fails_with_entries() {
    }

    #[test]
    fn test_get_entries() {
    }

    #[test]
    fn test_get_single_entry() {
    }

    #[test]
    fn test_unlink_entry() {
    }

    #[test]
    fn test_remove_entries() {
    }
}
