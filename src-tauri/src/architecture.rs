//! Architecture Enforcement Guards
//!
//! This module provides compile-time guarantees for the clean architecture.
//!
//! # Rules
//! 1. Repositories must implement `Repository` trait.
//! 2. Services must implement `Service` trait.
//! 3. Static guards enforce structural boundaries.

use std::marker::PhantomData;

/// Marker trait — all repository structs must implement this.
/// A repository must ONLY contain SQL and row-mapping.
/// No arithmetic, no cross-repo calls, no business logic.
pub trait Repository {}

/// Marker trait — all service structs must implement this.
/// A service orchestrates repositories. It must NOT contain SQL strings.
pub trait Service {}

/// Zero-cost guard: asserts at compile time that R: Repository
pub struct RepoGuard<R: Repository>(PhantomData<R>);

impl<R: Repository> Default for RepoGuard<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Repository> RepoGuard<R> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

/// Zero-cost guard: asserts at compile time that S: Service  
pub struct ServiceGuard<S: Service>(PhantomData<S>);

impl<S: Service> Default for ServiceGuard<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Service> ServiceGuard<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

// -----------------------------------------------------------------------------
// Guard Helpers
// -----------------------------------------------------------------------------

/// Explicitly asserts that a given type implements `Repository`.
#[inline(always)]
pub fn assert_repository<T: Repository>() {}

/// Explicitly asserts that a given type implements `Service`.
#[inline(always)]
pub fn assert_service<T: Service>() {}
