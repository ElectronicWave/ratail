use crate::redraw::Redraw;

/// Shared application services and the route type.
pub trait Context: Sized + Send + Sync + 'static {
    type Route: Clone + Send + Sync + 'static;

    /// Signal used by shared tasks to request a repaint.
    fn redraw(&self) -> &Redraw;
}
