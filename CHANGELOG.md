# Version 0.0.0

Initial codebase.

# Version 0.0.1

Changes:
* `Clone` + `Debug` is no longer required for members

# Version 0.0.2

Changes:
* Key type is now a generic type - no longer required to be `&str` [#2]
* All entries must implement `BumpyVector::AutoBumpyEntry` [#3]
* Entries are index with `std::ops::Range` instead of an `index` + `size` pair [#4]
