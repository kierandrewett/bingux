//! Where a table's rows come from.
//!
//! A table never copies its rows. It asks a [`RowSource`] how many there are
//! and for the one at a place, and only for the rows in view, so a source of a
//! hundred thousand rows costs what it costs to hold them. A `Vec`, an
//! `Rc<Vec>` and an `Rc<[T]>` are sources; a model that pages rows from a
//! database or a listing implements the trait.

use std::rc::Rc;

/// Rows by place.
pub trait RowSource<T> {
    fn len(&self) -> usize;

    /// The row at `index`, or `None` past the end.
    fn get(&self, index: usize) -> Option<&T>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> RowSource<T> for Vec<T> {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    fn get(&self, index: usize) -> Option<&T> {
        <[T]>::get(self, index)
    }
}

impl<T> RowSource<T> for Rc<[T]> {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    fn get(&self, index: usize) -> Option<&T> {
        <[T]>::get(self, index)
    }
}

/// A table's rows, shared and cheap to clone. Two values are equal when they
/// are the same allocation of the same length, so a caller that keeps its rows
/// in an `Rc` (in a hook or a state) does not rebuild the table's rows on every
/// render, and one that changes them makes a new `Rc`.
pub struct Rows<T> {
    source: Rc<dyn RowSource<T>>,
    /// The address of the allocation the source lives in.
    id: usize,
}

impl<T> Clone for Rows<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            id: self.id,
        }
    }
}

impl<T> PartialEq for Rows<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.source.len() == other.source.len()
    }
}

impl<T: 'static> Rows<T> {
    /// Rows from a source of your own, held in an `Rc`.
    pub fn from_source<S: RowSource<T> + 'static>(source: Rc<S>) -> Self {
        let id = Rc::as_ptr(&source) as *const () as usize;
        Self { source, id }
    }

    pub fn len(&self) -> usize {
        self.source.len()
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.source.get(index)
    }

    pub(crate) fn id(&self) -> usize {
        self.id
    }
}

impl<T: 'static> From<Vec<T>> for Rows<T> {
    /// A new allocation each time: build it once and keep it, or use an `Rc`.
    fn from(rows: Vec<T>) -> Self {
        Rows::from_source(Rc::new(rows))
    }
}

impl<T: 'static> From<Rc<Vec<T>>> for Rows<T> {
    fn from(rows: Rc<Vec<T>>) -> Self {
        Rows::from_source(rows)
    }
}

impl<T: 'static> From<Rc<[T]>> for Rows<T> {
    fn from(rows: Rc<[T]>) -> Self {
        let id = Rc::as_ptr(&rows) as *const () as usize;
        Self {
            source: Rc::new(rows),
            id,
        }
    }
}

impl<T: 'static> From<&Rc<Vec<T>>> for Rows<T> {
    fn from(rows: &Rc<Vec<T>>) -> Self {
        Rows::from_source(rows.clone())
    }
}
