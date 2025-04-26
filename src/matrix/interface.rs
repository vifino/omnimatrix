use super::model::*;
use anyhow::Result;
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
    fn get_router_matrix_info(
        &self,
        index: u32,
    ) -> impl Future<Output = Result<RouterMatrixInfo>> + Send + Sync;

    /// Get Input and Output Labels.
    ///
    /// This information may be cached depending on the implementation,
    /// but should definitely be made optional.
    fn get_labels(&self, index: u32) -> impl Future<Output = Result<RouterLabels>> + Send + Sync;

    /// Update Input and Output Labels.
    ///
    /// The provided changed labels will be merged with the existing labels.
    fn update_labels(
        &self,
        index: u32,
        changed: RouterLabels,
    ) -> impl Future<Output = Result<()>> + Send + Sync;

    /// Get currently patched routes.
    fn get_routes(&self, index: u32) -> impl Future<Output = Result<Vec<Patch>>> + Send + Sync;

    /// Update patched routes.
    ///
    /// The provided patches will update the existing patched routes.
    fn update_routes(
        &self,
        index: u32,
        changes: Vec<Patch>,
    ) -> impl Future<Output = Result<()>> + Send + Sync;

    // TODO: get/update locks?
    // TODO: alarms? settings?

    /// Subscribe to Events, creating a [futures_core::Stream].
    /// There is no explicit guarantee to get all events.
    ///
    /// This is the main way to get updates or changes happening outside of
    /// explicitly requesting them.
    fn event_stream(
        &self,
    ) -> impl Future<Output = Result<impl futures_core::Stream<Item = RouterEvent>>> + Send + Sync;
}
