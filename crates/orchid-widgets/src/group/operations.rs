//! Free-function helpers layered on top of [`super::GroupManager`].
//!
//! Today this module only exists to satisfy the module layout laid down in
//! the implementation spec; all operations are already implemented as
//! inherent methods on [`super::GroupManager`]. Future refactors can move
//! transactional helpers in here without shuffling the public API.

#[allow(unused_imports)]
pub(crate) use super::GroupManager;
