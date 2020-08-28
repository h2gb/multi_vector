# multi_vector

[![Crate](https://img.shields.io/crates/v/multi_vector.svg)](https://crates.io/crates/multi_vector)

An object that holds multiple `BumpyVector`s, and can manage linked entries
within a single vector, or between multiple vectors.

The purpose of this is to manage pointers and structs in `h2gb`. Sometimes
elements across disparate vectors (whether different layers, buffers, etc -
doesn't matter) need to be bundled together.

This is NOT for references, cross-references, base addresses, or keeping
track of logic within a binary. This is the wrong layer for that. I
struggled a lot to scope this jussst right, and I started finding that I
can't do too much here.

## Goal

[h2gb](https://github.com/h2gb/libh2gb) is a tool for analyzing binary
files. Some binary files will have multiple buffers (like sections in an
ELF file, files in a TAR file, etc.). Some of those will have a creator-
created relationship with each other, and we want to track that.

## Usage

Instantiate, add vectors, and add elements to the vectors. All elements
added together, as a "group", are linked, and will be removed together.

```rust
use multi_vector::MultiVector;

// Create an instance that stores Strings
let mut mv: MultiVector<&str, u32> = MultiVector::new();

// Create a pair of vectors, one that's 100 elements and one that's 200
mv.create_vector("myvector1", 100).unwrap();
mv.create_vector("myvector2", 200).unwrap();

// Vector names must be unique
assert!(mv.create_vector("myvector1", 100).is_err());

// It starts with zero entries
assert_eq!(0, mv.len());

// Populate it with one group
mv.insert_entries(vec![
    (&"myvector1", 111,  0, 10),
    (&"myvector1", 222, 10, 10),
]);

// Now there are two entries
assert_eq!(2, mv.len());

// Populate with some more values
mv.insert_entries(vec![
    (&"myvector1", 111,  20, 10),
    (&"myvector2", 222,  0,  10),
    (&"myvector2", 222,  10, 10),
]);

// Now there are five entries!
assert_eq!(5, mv.len());

// Remove en entry from the first group, note that both entries get
// removed
assert_eq!(2, mv.remove_entries(&"myvector1", 15).unwrap().len());
assert_eq!(3, mv.len());

// myvector1 still has an entry, so we can't remove it
assert!(mv.destroy_vector(&"myvector1").is_err());

// Split the final "myvector1" entry out of the group
assert!(mv.unlink_entry(&"myvector1", 20).is_ok());

// Remove the final "myvector1" entry.. since we unlinked it, it'll remove
// alone
assert_eq!(1, mv.remove_entries(&"myvector1", 20).unwrap().len());

// Now there are just two elements left, both in "myvector2"
assert_eq!(2, mv.len());

// Now we can remove myvector1, since it's empty
assert_eq!(100, mv.destroy_vector(&"myvector1").unwrap());
```

## Serialize / deserialize

When installed with the 'serialize' feature:

```toml
multi_vector = { version = "~0.0.0", features = ["serialize"] }
```

Serialization support using [serde](https://serde.rs/) is enabled. The
`MultiVector` can be serialized with any of the serializers that Serde
supports, such as [ron](https://github.com/ron-rs/ron):

```rust
use multi_vector::MultiVector;

// Assumes "serialize" feature is enabled: `multi_vector = { features = ["serialize"] }`
let mut mv: MultiVector<&str, String> = MultiVector::new();
mv.create_buffer("buf", 100);
mv.insert_entries(vec![
    (&"buf", String::from("a"),  0, 10),
    (&"buf", String::from("B"), 10, 10),
]);

// Serialize
let serialized = ron::ser::to_string(&mv).unwrap();

// Deserialize
let mv: MultiVector<String> = ron::de::from_str(&serialized).unwrap();

assert_eq!(2, mv.len());
```

License: MIT
