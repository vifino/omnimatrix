use super::model::*;
use anyhow::Result;
use futures_core::stream::BoxStream;
use std::future::Future;

/// Matrix Router Abstraction.
///
/// The interface is minimal and only supports a small subset of commands.
/// This is to ensure it is applicable to most routers.
///
/// Some information might be wise to cache, but it's the implementation's choice whether to do so.
/// Caching some information might result in outdated information being returned if the router is
/// being controlled outside of this instance. A setting might be wise.
pub trait MatrixRouter: Send + Sync {
    /// Return whether or not the Router is assumed connected.
    ///
    /// This might be cached and only updated once a communication failure occured or
    /// implemented as a ping message.
    fn is_alive(&self) -> impl Future<Output = Result<bool>> + Send + Sync;

    /// Get general Router Info.
    ///
    /// This information generally should not change too frequently
    /// and might be cached.
    fn get_router_info(&self) -> impl Future<Output = Result<RouterInfo>> + Send + Sync;

    /// Get Router Matrix Info.
    ///
    /// This information generally should not change too frequently
    /// and might be cached.
    fn get_matrix_info(
        &self,
        index: u32,
    ) -> impl Future<Output = Result<RouterMatrixInfo>> + Send + Sync;

    /// Get Input Labels.
    ///
    /// This information may be cached depending on the implementation,
    /// but should definitely be made optional.
    fn get_input_labels(
        &self,
        index: u32,
    ) -> impl Future<Output = Result<Vec<RouterLabel>>> + Send + Sync;

    /// Get Output Labels.
    ///
    /// This information may be cached depending on the implementation,
    /// but should definitely be made optional.
    fn get_output_labels(
        &self,
        index: u32,
    ) -> impl Future<Output = Result<Vec<RouterLabel>>> + Send + Sync;

    /// Update Input Labels.
    ///
    /// The provided changed labels will be merged with the existing labels.
    fn update_input_labels(
        &self,
        index: u32,
        changed: Vec<RouterLabel>,
    ) -> impl Future<Output = Result<()>> + Send + Sync;

    /// Update Output Labels.
    ///
    /// The provided changed labels will be merged with the existing labels.
    fn update_output_labels(
        &self,
        index: u32,
        changed: Vec<RouterLabel>,
    ) -> impl Future<Output = Result<()>> + Send + Sync;

    /// Get currently patched routes.
    fn get_routes(
        &self,
        index: u32,
    ) -> impl Future<Output = Result<Vec<RouterPatch>>> + Send + Sync;

    /// Update patched routes.
    ///
    /// The provided patches will update the existing patched routes.
    fn update_routes(
        &self,
        index: u32,
        changes: Vec<RouterPatch>,
    ) -> impl Future<Output = Result<()>> + Send + Sync;

    // TODO: get/update locks?
    // TODO: alarms? settings?

    /// Subscribe to Events, creating a [futures_core::Stream].
    /// There is no explicit guarantee to get all events.
    ///
    /// This is the main way to get updates or changes happening outside of
    /// explicitly requesting them.
    fn event_stream<'a>(
        &'a self,
    ) -> impl Future<Output = Result<BoxStream<'a, RouterEvent>>> + Send + Sync;
}
