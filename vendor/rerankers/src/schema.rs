//! Schema types used to represent documents and their scores.

use std::borrow::Cow;

/// A document to be reranked.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    /// The unique identifier of the document.
    pub id: usize,
    /// The text content of the document.
    pub text: Cow<'a, str>,
}

impl<'a> Document<'a> {
    /// Creates a new document with the given id and text.
    pub fn new(id: usize, text: impl Into<Cow<'a, str>>) -> Self {
        let text = text.into();
        Self { id, text }
    }
}

/// A score assigned to a document by a reranker.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Score(pub f64);
