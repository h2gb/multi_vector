use std::collections::HashMap;
use bumpy_vector::{BumpyVector, BumpyEntry};
use std::fmt::Debug;

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
    fn wrap_entry(vector: String, entry: BumpyEntry<T>, friends: Vec<(String, usize)>) -> BumpyEntry<MultiEntry<T>> {
        BumpyEntry {
            entry: MultiEntry {
                vector: vector,
                friends: friends,
                data: entry.entry,
            },
            index: entry.index,
            size: entry.size,
        }
    }

    fn unwrap_entry(entry: BumpyEntry<MultiEntry<T>>) -> (String, BumpyEntry<T>, Vec<(String, usize)>) {
        let vector = entry.entry.vector;
        let data = entry.entry.data;
        let friends = entry.entry.friends;

        (vector, BumpyEntry { entry: data, index: entry.index, size: entry.size }, friends)
    }
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

    pub fn create_vector(&mut self, name: &str, max_size: usize) -> Result<(), &'static str> {
        if self.vectors.contains_key(name) {
            return Err("Vector with that name already exists");
        }

        self.vectors.insert(String::from(name), BumpyVector::new(max_size));

        Ok(())
    }

    pub fn destroy_vector(&mut self, vector: &str) -> Result<usize, &'static str> {
        let v = match self.vectors.get(vector) {
            Some(v) => v,
            None => return Err("Vector with that name does not exist"),
        };

        if v.len() != 0 {
            return Err("Vector is not empty");
        }

        match self.vectors.remove(vector) {
            Some(v) => Ok(v.max_size()),
            None    => Err("Vector with that name disappeared"),
        }
    }

    pub fn insert_entries(&mut self, entries: Vec<(String, BumpyEntry<T>)>) -> Result<(), String> {
        // Clone the full set so we can backtrack if things go wrong
        // XXX: This is JUST for testing! This is incredibly slow!
        let backtrack = self.vectors.clone();

        // Get the set of references that each entry will store - the vector and
        // location of reach
        let references: Vec<(String, usize)> = entries.iter().map(|(vector, entry)| {
            (vector.clone(), entry.index)
        }).collect();

        println!("==");
        println!("References: {:?}", references);
        println!("==");

        for (vector, entry) in entries {
            // Try and get a handle to the vector
            let v = match self.vectors.get_mut(&vector) {
                Some(v) => v,
                None => {
                    // Remove the entries we've added so far + return error
                    self.vectors = backtrack;
                    return Err(format!("Couldn't find vector: {}", vector));
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
                    return Err(format!("Error inserting into vector: {}", e));
                }
            }
        }

        Ok(())
    }

    fn _remove_entry(&mut self, vector: &str, address: usize) -> Option<BumpyEntry<MultiEntry<T>>> {
        let v = match self.vectors.get_mut(vector) {
            Some(v) => v,
            None => return None,
        };

        return v.remove(address);
    }

    pub fn remove_entries(&mut self, vector: &str, address: usize) -> Result<Vec<BumpyEntry<T>>, &'static str> {
        let (v, e, f) = match self._remove_entry(vector, address) {
            Some(e) => MultiEntry::unwrap_entry(e),
            None => return Err("Could not find entry"),
        };

        Ok(vec![])
    }

    // // Remove from a group
    // pub fn unlink_entry(_vector: &str, _address: usize) {
    // }

    // pub fn get_single(_vector: String, _address: usize) -> Option<MultiEntry<&'a T>> {
    //     None
    // }

    // pub fn get_group(_vector: String, _address: usize) -> Option<Vec<MultiEntry<&'a T>>> {
    //     None
    // }

    // pub fn len_vector(_vector: &str) {
    // }

    // pub fn len() -> usize {
    //     0
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_test() -> Result<(), String> {
        let mut test: MultiVector<u32> = MultiVector::new();
        println!("{:?}", test);

        test.create_vector("test", 100);

        let mut entries: Vec<(String, BumpyEntry<u32>)> = vec![
            ("test".into(), (123, 0, 1).into()),
            ("test".into(), (123, 5, 1).into()),
            ("test".into(), (123, 10, 1).into()),
            ("test".into(), (123, 11, 1).into()),
            ("test".into(), (123, 20, 1).into()),
        ];

        println!("Before: {:?}", test);
        println!();

        match test.insert_entries(entries) {
            Ok(()) => println!(" ** OK **"),
            Err(e) => println!("ERR: {:?}", e),
        }
        println!();
        println!("After: {:?}", test);
        println!();

        Ok(())
    }
}
