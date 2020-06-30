use std::collections::HashMap;
use bumpy_vector::{BumpyVector, BumpyEntry};

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
    vector: String,
    entry: BumpyEntry<T>,
}

impl<T> From<(String, BumpyEntry<T>)> for MultiEntry<T> {
    fn from(o: (String, BumpyEntry<T>)) -> Self {
        MultiEntry {
          vector: o.0,
          entry: o.1,
        }
    }
}

impl<T> From<(String, T, usize, usize)> for MultiEntry<T> {
    fn from(o: (String, T, usize, usize)) -> Self {
        MultiEntry {
          vector: o.0,
          entry: BumpyEntry::from((o.1, o.2, o.3)),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
pub struct MultiVector<T>
where
    T: Clone
{
    // A map of bumpy_vectors, indexed by name
    vectors: HashMap<String, BumpyVector<T>>,
}

impl<'a, T> MultiVector<T>
where
    T: Clone
{

    pub fn new() -> Self {
        MultiVector {
            vectors: HashMap::new(),
        }
    }

    pub fn insert_vector(&mut self, name: &str, vector: BumpyVector<T>) -> Result<(), &'static str> {
        if self.vectors.contains_key(name) {
            return Err("Vector with that name already exists");
        }

        self.vectors.insert(String::from(name), vector);

        Ok(())
    }

    pub fn remove_vector(&mut self, vector: &str) -> Result<BumpyVector<T>, &'static str> {
        let v = match self.vectors.get(vector) {
            Some(v) => v,
            None => return Err("Vector with that name does not exist"),
        };

        if v.len() != 0 {
            return Err("Vector is not empty");
        }

        self.vectors.remove(vector).ok_or("Vector with that name disappeared")
    }

    pub fn insert_entries(&mut self, entries: Vec<MultiEntry<T>>) -> Result<(), String> {
        // Clone the full set so we can backtrack if things go wrong
        // XXX: This is JUST for testing! This is incredibly slow!
        let backtrack = self.vectors.clone();

        for entry in entries {
            // Try and get a handle to the vector
             let v = match self.vectors.get_mut(&entry.vector) {
                 Some(v) => v,
                 None => {
                     // Remove the entries we've added so far + return error
                     self.vectors = backtrack;
                     return Err(format!("Couldn't find vector: {}", entry.vector));
                 }
             };

             // Try and insert it into the BumpyVector
             match v.insert(entry.entry) {
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

    pub fn remove_entries(_vector: &str, _address: usize) -> Vec<MultiEntry<T>> {
        Vec::new()
    }

    // Remove from a group
    pub fn unlink_entry(_vector: &str, _address: usize) {
    }

    pub fn get_single(_vector: String, _address: usize) -> Option<MultiEntry<&'a T>> {
        None
    }

    pub fn get_group(_vector: String, _address: usize) -> Option<Vec<MultiEntry<&'a T>>> {
        None
    }

    pub fn len_vector(_vector: &str) {
    }

    pub fn len() -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_test() -> Result<(), String> {
        let mut test: MultiVector<u32> = MultiVector::new();
        println!("{:?}", test);

        test.insert_vector("test", BumpyVector::new(100));

        let mut entries: Vec<MultiEntry<u32>> = vec![
            ("test".into(), 123, 0, 1).into(),
            ("test".into(), 123, 1, 1).into(),
            ("test".into(), 123, 2, 1).into(),
            ("test".into(), 123, 3, 1).into(),
            ("test".into(), 123, 4, 1).into(),
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
